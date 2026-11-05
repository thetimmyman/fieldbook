//! Seeded no-panic test for `Ledger::parse` (`core-labels` step 7).

#[path = "../../../tests/support/rng.rs"]
mod rng;

use fieldbook_core::labels::Ledger;
use rng::Rng;

#[test]
fn parser_never_panics_on_200_seeded_inputs() {
    let mut rng = Rng::new(1);
    for i in 0..200usize {
        let len = i % 129;
        let raw = rng.bytes(len);
        let text = String::from_utf8_lossy(&raw);
        let _ = Ledger::parse(&text);
    }

    let valid = "\
seq,item_id,kind,labels,origin
1,SO-000042-v1,add,np-0007,person
2,CM-000012-v1,add,bid-0003|np-0007,intake
3,SO-000042-v1,add,ctl-0002,reapply
";
    assert!(Ledger::parse(valid).is_ok());

    let valid_bytes = valid.as_bytes();
    for idx in 0..valid_bytes.len() {
        let mut flipped = valid_bytes.to_vec();
        flipped[idx] ^= 0x5a;
        let flipped_str = String::from_utf8_lossy(&flipped);
        let _ = Ledger::parse(&flipped_str);

        let mut deleted = valid_bytes.to_vec();
        deleted.remove(idx);
        let deleted_str = String::from_utf8_lossy(&deleted);
        let _ = Ledger::parse(&deleted_str);
    }
}
