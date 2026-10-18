"""Audio sink: the samples the Rust core collected from McBSP0 (core.take_audio()) go to
16-bit stereo WAV files. A change of the sample rate starts a new file (NAME.wav,
NAME-1.wav, ...); the header carries the rate rounded to whole Hz (the AIC23's
44.118 kHz -> 44118).
"""

import wave
from pathlib import Path


class WavRecorder:
    def __init__(self, path):
        self.path = Path(path)
        self.files = []                 # paths written
        self.frames = 0
        self._wav = None
        self._rate = None

    def feed(self, segments):
        """segments: [(rate, little-endian int16 interleaved L/R bytes)]."""
        for rate, data in segments:
            if rate != self._rate:
                self._open(rate)
            self._wav.writeframes(data)
            self.frames += len(data) // 4

    def _open(self, rate):
        self.close()
        n = len(self.files)
        path = self.path if n == 0 else self.path.with_name(f"{self.path.stem}-{n}{self.path.suffix}")
        self._wav = wave.open(str(path), "wb")
        self._wav.setnchannels(2)
        self._wav.setsampwidth(2)
        self._wav.setframerate(round(rate))
        self._rate = rate
        self.files.append(path)

    def close(self):
        if self._wav is not None:
            self._wav.close()
            self._wav = None
