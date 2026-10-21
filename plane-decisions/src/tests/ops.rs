use super::*;

#[test]
fn row_for_matches_systemone() {
    let row = row_for("POST", PATH_SYSTEMONE).expect("systemone is declared");
    assert_eq!(row.op, OP_SYSTEMONE);
    assert!(row.has_request_body);
}

#[test]
fn row_for_is_verb_case_insensitive() {
    assert!(row_for("post", PATH_SYSTEMONE).is_some());
    assert!(row_for("post", PATH_EVALUATE).is_some());
}

#[test]
fn row_for_refuses_wrong_verb() {
    assert!(row_for("GET", PATH_SYSTEMONE).is_none());
    assert!(row_for("GET", PATH_EVALUATE).is_none());
}

#[test]
fn row_for_refuses_unknown_path() {
    assert!(row_for("GET", "/v1/unknown").is_none());
}

/// `GET /v1/models` is no operation of this plane: the path keeps busbar's model list.
#[test]
fn the_model_list_is_no_operation_of_this_plane() {
    assert!(row_for("GET", "/v1/models").is_none());
}

/// The method table names exactly the op classes the Statement tail states, in its order.
#[test]
fn the_method_table_is_the_tails_op_classes() {
    let ops: Vec<_> = METHODS.iter().map(|r| r.op).collect();
    assert_eq!(ops, crate::driven::tail::OP_CLASSES);
}

#[test]
fn row_for_matches_evaluate() {
    let row = row_for("POST", PATH_EVALUATE).expect("evaluate is declared");
    assert_eq!(row.op, OP_EVALUATE);
    assert!(row.has_request_body);
}
