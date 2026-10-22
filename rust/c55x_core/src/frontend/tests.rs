//! The front end against ir.py and listing.py: the expected values are the Python repr()
//! of what the Python front end returns (or the text of the exception it raises) for the
//! same input. tools/frontend_ir_check.py compares the two front ends on whole corpora.

use std::collections::HashMap;

use super::listing::{block_end, parse};
use super::{build_text, resolver};
use crate::ast::Ast;
use crate::err::Error;

/// Python repr() of the IR.
fn repr(a: &Ast) -> String {
    let all = |v: &[Ast]| v.iter().map(repr).collect::<Vec<_>>().join(", ");
    match a {
        Ast::None => "None".into(),
        Ast::Bool(b) => if *b { "True" } else { "False" }.into(),
        Ast::Int(v) => v.to_string(),
        Ast::Str(s) => format!("'{}'", s),
        Ast::List(v) => format!("[{}]", all(v)),
        Ast::Tuple(v) if v.len() == 1 => format!("({},)", all(v)),
        Ast::Tuple(v) => format!("({})", all(v)),
    }
}

fn message(e: Error) -> String {
    match e {
        Error::Emu(m) | Error::Unsupported(m) => m,
        Error::Host(_) => "host error".into(),
    }
}

fn build(text: &str, block_end: Option<i64>) -> String {
    let symbols = HashMap::from([("L1".to_owned(), 0x20100)]);
    let resolve = resolver(&symbols);
    match build_text(text, 0x20000, 2, block_end, &resolve) {
        Ok(ir) => repr(&ir),
        Err(e) => format!("ERR {}", message(e)),
    }
}

const NO_SKIP: &str = "None, None, (False, []))";

#[test]
fn builds_like_ir_py() {
    let cases: &[(&str, Option<i64>, &str)] = &[
        ("AC0 = *AR3", None, "[(False, ('assign', ('reg', 'AC0'), ('mem', 'ind', 'AR3', None)))], None, NO_SKIP"),
        ("AC0 = *AR3(#0002h)", None, "[(False, ('assign', ('reg', 'AC0'), ('mem', 'idx', 'AR3', 2)))], None, NO_SKIP"),
        ("AC1 = |AC0|", None, "[(False, ('assign', ('reg', 'AC1'), ('call', 'abs', [('reg', 'AC0')])))], None, NO_SKIP"),
        ("AC0 = mant(AC1), T1 = exp(AC1)", None,
         "[(False, ('assign', ('reg', 'AC0'), ('call', 'mant', [('reg', 'AC1')]))), \
          (False, ('assign', ('reg', 'T1'), ('call', 'mant_exp', [('reg', 'AC1')])))], None, NO_SKIP"),
        ("AC0 = AC0 + (*AR2 * coef(*CDP+)), AC1 = AC1 + (*AR3 * coef(*CDP+))", None,
         "[(False, ('assign', ('reg', 'AC0'), ('bin', '+', ('reg', 'AC0'), ('bin', '*', \
          ('mem', 'ind', 'AR2', None), ('call', 'coef', [('mem', 'ind', 'CDP', None)]))))), \
          (False, ('assign', ('reg', 'AC1'), ('bin', '+', ('reg', 'AC1'), ('bin', '*', \
          ('mem', 'ind', 'AR3', None), ('call', 'coef', [('mem', 'post', 'CDP', 1)])))))], \
          None, None, None, (True, []))"),
        ("TC1 = bit(*AR3, #2), bit(*AR3, #2) = #1", None,
         "[(False, ('test_set', 'TC1', ('mem', 'ind', 'AR3', None), 2, 1))], None, NO_SKIP"),
        ("if (AC0 < #0) goto #0x020000", None,
         "[], ('goto', ('bin', '<', ('reg', 'AC0'), ('num', 0)), ('abs', 131072)), NO_SKIP"),
        ("call L1", None, "[], ('call', None, ('abs', 131328), 131074), NO_SKIP"),
        ("while (TC1 && (RPTC < #5)) repeat", None,
         "[], None, None, (5, ('reg', 'TC1'), 100), (False, []))"),
        ("localrepeat {", Some(0x20010), "[(False, ('loop', 'local', 131074, 131088))], None, NO_SKIP"),
        ("mar(*AR3+) || AC0 = AC0 + AR3", None,
         "[(True, ('mar', ('call', 'mar', [('mem', 'post', 'AR3', 1)]))), \
          (False, ('assign', ('reg', 'AC0'), ('bin', '+', ('reg', 'AC0'), ('reg', 'AR3'))))], \
          None, None, None, (False, [0]))"),
        ("if (TC2) execute(D_Unit) || *AR3+ = AC1", None,
         "[(False, ('assign', ('mem', 'post', 'AR3', 1), ('reg', 'AC1')))], None, \
          (('reg', 'TC2'), 3), None, (True, []))"),
    ];
    for (text, end, expect) in cases {
        let expect = expect.replace("NO_SKIP", NO_SKIP);
        assert_eq!(build(text, *end), format!("('insn', False, False, False, {}", expect), "{}", text);
    }
    assert_eq!(build("T1 = @AR3_L || mmap()", None),
               "('insn', True, False, False, [(False, ('assign', ('reg', 'T1'), ('mmr', 19)))], None, None, None, (False, []))");
}

#[test]
fn rejects_like_ir_py() {
    let cases = [
        ("if (TC1) return_int", "conditional return_int"),
        ("repeat(#010)", "invalid literal for int() with base 0: '010'"),
        ("AC0 = AC0 ? AC1", "trailing text at 3 in 'AC0 ? AC1'"),
        ("goto L9", "unknown branch target L9"),
        ("AC0 = @NOPE_L || mmap()", "memory-mapped register @NOPE_L"),
        ("mar(*AR3+) || AC0 = *AR3 + AR3", "AR3 used as address and value next to mar()"),
    ];
    for (text, why) in cases {
        assert_eq!(build(text, None), format!("ERR {}", why), "{}", text);
    }
}

#[test]
fn block_end_from_the_encoding() {
    let end = |addr, raw: &[u8], text| block_end(addr, raw, text).map_err(message);
    assert_eq!(end(0x100, &[0x4a, 0x83], "localrepeat {").ok(), Some(Some(0x105)));
    assert_eq!(end(0x100, &[0x22, 0x01, 0x4b, 0x83], "AC1 = AC0 || localrepeat {").ok(), Some(Some(0x107)));
    assert_eq!(end(0x100, &[0x0e, 0x00, 0x05], "blockrepeat {").ok(), Some(Some(0x108)));
    assert_eq!(end(0x100, &[0x40, 0x1b, 0x0f, 0x00, 0x03], "AR3 = AR3 + #1 || blockrepeat {").ok(),
               Some(Some(0x108)));
    assert_eq!(end(0x100, &[0x20], "nop").ok(), Some(None));
    assert_eq!(end(0x100, &[0x20, 0x00], "localrepeat {").err().unwrap(),
               "0x000100: cannot locate the repeat opcode in 2000 'localrepeat {'");
}

const LISTING: &str = "\t.sect \".text\"
020000: 4a83             localrepeat {
020002: d66300              AC0 = AC0 + *AR3+
020005: d68311              AC1 = AC1 + *AR4+
                         }
020008: 2201_4b83        AC1 = AC0 || localrepeat {
02000c: d66300              AC0 = AC0 + *AR3+
02000f: d68311              AC1 = AC1 + *AR4+
                         }
020012: 7a00000c         idle
";

#[test]
fn reads_a_dis55_listing() {
    let insns = parse(LISTING, true).map_err(message).ok().unwrap();
    let rows: Vec<_> = insns.values().map(|i| (i.addr, i.size, i.text.as_str(), i.block_end)).collect();
    assert_eq!(rows, [
        (0x20000, 2, "localrepeat {", Some(0x20005)),
        (0x20002, 3, "AC0 = AC0 + *AR3+", None),
        (0x20005, 3, "AC1 = AC1 + *AR4+", None),
        (0x20008, 4, "AC1 = AC0 || localrepeat {", Some(0x2000f)),
        (0x2000c, 3, "AC0 = AC0 + *AR3+", None),
        (0x2000f, 3, "AC1 = AC1 + *AR4+", None),
        (0x20012, 4, "idle", None),
    ]);
}

#[test]
fn listing_errors_like_listing_py() {
    let err = |text: &str, markers| parse(text, markers).map_err(message).err();
    let cut = LISTING.replace("02000f: d68311              AC1 = AC1 + *AR4+\n", "");
    assert_eq!(err(&cut, true).unwrap(), "0x020008: encoded block end 0x02000f differs from dis55 0x02000c");
    assert_eq!(err(&cut, false), None);
    assert_eq!(err(&format!("{}}}\n", LISTING), true).unwrap(), "unbalanced '}' after 0x020012");
    let open = LISTING.rsplit_once('}').unwrap().0;
    assert_eq!(err(open, true).unwrap(), "unterminated repeat block at 0x020008");
}

// ---- tests converted from tests/*.py (expected values as the Python tests had them) ----

fn parsed(text: &str) -> String {
    super::expr::parse(text).map(|e| repr(&e.to_ast())).map_err(message).unwrap()
}

/// test_c55x_emu.py ParseTests and test_c55x_emu_addressing.py ShortOperandTests.
mod parse {
    use super::parsed;

    #[test]
    fn numbers() {
        let got: Vec<i64> = ["#8191", "#-1", "#0x020120", "#01600h"].iter()
            .map(|s| super::super::expr::number(s).ok().unwrap()).collect();
        assert_eq!(got, [8191, -1, 0x20120, 0x1600]);
    }

    #[test]
    fn memory_operands() {
        assert_eq!(parsed("*+AR2(#0001h)"), "('mem', 'pre', 'AR2', 1)");
        assert_eq!(parsed("*(AR3+T0)"), "('mem', 'postreg', 'AR3', ('+', 'T0'))");
        assert_eq!(parsed("*AR3(#009fh)"), "('mem', 'idx', 'AR3', 159)");
        assert_eq!(parsed("*AR3+"), "('mem', 'post', 'AR3', 1)");
    }

    #[test]
    fn multiply_is_not_a_memory_operand() {
        assert_eq!(parsed("AC0 * T0"), "('bin', '*', ('reg', 'AC0'), ('reg', 'T0'))");
    }

    #[test]
    fn short_operand() {
        assert_eq!(parsed("*AR1(short(#3))"), "('mem', 'short', 'AR1', 3)");
    }
}

/// test_c55x_emu_dsp.py MantExpTests (the statement split of ir.py _dual).
mod dual {
    use super::super::ir::dual;

    #[test]
    fn split_indexed_coefficient() {
        let got = dual("AC0 = (AC0 >> #16) + (*AR2 * coef(*(CDP+T0))), \
                        AC1 = (AC1 >> #16) + (*AR3 * coef(*(CDP+T0)))").ok().unwrap();
        assert_eq!(got, ["AC0 = (AC0 >> #16) + (*AR2 * coef(*CDP))",
                         "AC1 = (AC1 >> #16) + (*AR3 * coef(*(CDP+T0)))"]);
    }

    #[test]
    fn split() {
        assert_eq!(dual("AC2 = mant(AC0), T0 = exp(AC0)").ok().unwrap(), ["AC2 = mant(AC0)", "T0 = mant_exp(AC0)"]);
    }
}

/// (was tests/test_c55x_frontend.py) The synthetic program of rust/fixtures/frontend_synthetic
/// (asm55 / dis55 -qq -r -g, c55x-fixtures) against the outputs of listing.py and ir.py
/// frozen in ir.tsv, rejected.tsv and fuzz.tsv.
mod synthetic {
    use std::collections::HashMap;

    use super::{message, repr};
    use crate::frontend::listing::parse;
    use crate::frontend::{build_text, resolver};

    macro_rules! fixture {
        ($file:literal) => {
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../fixtures/frontend_synthetic/", $file))
        };
    }
    const LISTING: &str = fixture!("prog.lst");

    /// The rows of a .tsv fixture after its one-line '#' header.
    fn rows(text: &str) -> impl Iterator<Item = Vec<&str>> {
        assert!(text.starts_with('#'));
        text.lines().skip(1).map(|l| l.split('\t').collect())
    }

    fn hex(v: &str) -> i64 {
        i64::from_str_radix(v.trim_start_matches("0x"), 16).unwrap()
    }

    fn outcome(text: &str, addr: i64, size: i64, block_end: Option<i64>, symbols: &HashMap<String, i64>) -> String {
        let resolve = resolver(symbols);
        match build_text(text, addr, size, block_end, &resolve) {
            Ok(ir) => repr(&ir),
            Err(e) => format!("ERR {}", message(e)),
        }
    }

    #[test]
    fn listing_readers_agree() {
        let expect: Vec<String> = rows(fixture!("ir.tsv")).map(|r| r[..4].join("\t")).collect();
        for markers in [true, false] {
            let got: Vec<String> = parse(LISTING, markers).map_err(message).ok().unwrap().values()
                .map(|i| format!("0x{:06x}\t{}\t{}\t{}", i.addr, i.size,
                                 i.block_end.map_or("-".into(), |e| format!("0x{:06x}", e)), i.text))
                .collect();
            assert_eq!(got, expect, "markers {}", markers);
        }
    }

    #[test]
    fn listing_errors_agree() {
        let err = |text: &str| parse(text, true).map_err(message).err().unwrap();
        let cut = LISTING.replace("0200c6: d68311              AC1 = AC1 + *AR4+\n", "");
        assert_eq!(err(&cut), "0x0200bf: encoded block end 0x0200c6 differs from dis55 0x0200c3");
        assert_eq!(err(&format!("{}\n}}", LISTING)), "unbalanced '}' after 0x020118");
        assert_eq!(err(LISTING.rsplit_once('}').unwrap().0), "unterminated repeat block at 0x0200d3");
    }

    #[test]
    fn synthetic_listing() {
        let none = HashMap::new();
        let mut n = 0;
        for r in rows(fixture!("ir.tsv")) {
            let end = (r[2] != "-").then(|| hex(r[2]));
            assert_eq!(outcome(r[3], hex(r[0]), r[1].parse().unwrap(), end, &none), r[4], "{}", r[3]);
            assert!(!r[4].starts_with("ERR"), "{}", r[3]);
            n += 1;
        }
        assert_eq!(n, 98);
    }

    #[test]
    fn rejections() {
        let symbols = HashMap::from([("L1".to_owned(), 0x20010)]);
        let mut n = 0;
        for r in rows(fixture!("rejected.tsv")) {
            assert_eq!(outcome(r[0], 0x20000, 2, None, &symbols), r[1], "{}", r[0]);
            n += 1;
        }
        assert_eq!(n, 15);
    }

    #[test]
    fn fuzz() {
        let none = HashMap::new();
        let labels = HashMap::from([("L1".to_owned(), 0x1234), ("L2".to_owned(), 0x20000)]);
        let mut n = 0;
        for r in rows(fixture!("fuzz.tsv")) {
            let end = (r[3] != "-").then(|| hex(r[3]));
            let symbols = if r[4] == "1" { &labels } else { &none };
            assert_eq!(outcome(r[0], hex(r[1]), r[2].parse().unwrap(), end, symbols), r[5], "{}", r[0]);
            n += 1;
        }
        assert_eq!(n, 3000);
    }
}
