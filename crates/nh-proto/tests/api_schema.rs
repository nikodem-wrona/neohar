//! The snapshot of the whole contract. A change of the API shows up here as a diff, and the
//! reviewer reads it. Within `neohar/1` a change only adds.

#![cfg(feature = "api")]

use nh_proto::api::schema::document;

#[test]
fn the_contract_matches_the_snapshot() {
    insta::with_settings!({sort_maps => true}, {
        insta::assert_json_snapshot!("contract", document());
    });
}
