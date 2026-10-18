"""Live audio for the Qt front end: the samples the Rust core collected from McBSP0
(machine.audio_out.feed, same interface as wav.WavRecorder) go to the system audio
output through QtMultimedia (push mode).

The emulation is paced to real time, so the output only needs a small cushion: playback
starts once PREFILL_S of audio is queued. A rate change (the codec's 44.118 kHz is
played as 44118 Hz) reopens the output. While the UI runs in fast forward the samples
are discarded (pause()), because they arrive faster than they can be played.

gains() -> (left, right): the codec's headphone gain (codec.Codec.gains), applied to the
samples before they are queued, so the player's volume buttons work; results beyond the
16-bit range clip as the output stage would.
"""

import sys
from array import array

from PyQt5.QtMultimedia import QAudio, QAudioDeviceInfo, QAudioFormat, QAudioOutput

PREFILL_S = 0.15
BYTES_PER_FRAME = 4                     # 16-bit stereo


def scaled(data, left, right):
    """Little-endian int16 L/R bytes times the channel gains, clipped to 16 bits."""
    if left == right == 1.0:
        return data
    s = array("h", data)
    if sys.byteorder != "little":
        s.byteswap()
    for i, g in ((0, left), (1, right)):
        s[i::2] = array("h", (max(-32768, min(32767, round(v * g))) for v in s[i::2]))
    if sys.byteorder != "little":
        s.byteswap()
    return s.tobytes()


class AudioSink:
    def __init__(self, gains):
        self.gains = gains
        self.out = self.dev = None
        self.rate = None
        self.pending = bytearray()
        self.paused = False
        self.underruns = 0

    def feed(self, segments):
        """segments: [(rate, little-endian int16 interleaved L/R bytes)]."""
        if self.paused:
            return
        for rate, data in segments:
            if rate != self.rate:
                self._open(rate)
            self.pending += scaled(data, *self.gains())
        self.push()

    def _open(self, rate):
        self.close()
        fmt = QAudioFormat()
        fmt.setSampleRate(round(rate))
        fmt.setChannelCount(2)
        fmt.setSampleSize(16)
        fmt.setCodec("audio/pcm")
        fmt.setByteOrder(QAudioFormat.LittleEndian)
        fmt.setSampleType(QAudioFormat.SignedInt)
        info = QAudioDeviceInfo.defaultOutputDevice()
        if not info.isFormatSupported(fmt):
            raise RuntimeError(f"audio output {info.deviceName()!r} does not take {round(rate)} Hz "
                               "16-bit stereo PCM")
        self.out = QAudioOutput(info, fmt)
        self.out.setBufferSize(int(rate * PREFILL_S * 2) * BYTES_PER_FRAME)
        self.out.stateChanged.connect(self._state)
        self.rate = rate
        self.dev = None
        print(f"audio: {round(rate)} Hz stereo on {info.deviceName()}", file=sys.stderr)

    def push(self):
        """Hand queued samples to the output (also called from the UI timer)."""
        if self.out is None:
            return
        if self.dev is None:
            if len(self.pending) < self.rate * PREFILL_S * BYTES_PER_FRAME:
                return
            self.dev = self.out.start()
        free = self.out.bytesFree() // BYTES_PER_FRAME * BYTES_PER_FRAME
        if free and self.pending:
            n = self.dev.write(bytes(self.pending[:free]))
            if n > 0:
                del self.pending[:n]

    def _state(self, state):
        if state == QAudio.IdleState and self.out.error() == QAudio.UnderrunError:
            self.underruns += 1             # the emulation fell behind; resumes on the next write

    def pause(self, on):
        """Fast forward: drop samples instead of queueing them."""
        self.paused = on
        if on:
            self.pending.clear()

    def close(self):
        if self.out is not None:
            self.out.stop()
        self.out = self.dev = None
        self.rate = None
