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
