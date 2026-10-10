#[test]
fn modified_keys_get_the_engines_names() {
    use bn_client_core::keys::{chord, function, numpad};
    assert_eq!(chord("s", true, false), "CTRL+S");
    assert_eq!(chord("TAB", false, true), "BACKTAB");
    assert_eq!(chord("TAB", false, false), "TAB");
    assert_eq!(chord("UP", true, false), "UP", "only letters take CTRL+");
    assert_eq!(function(4), "F4");
    assert_eq!(numpad("8").as_deref(), Some("NUMPAD_8"));
    assert_eq!(numpad("x"), None);
}
