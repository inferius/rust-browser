/// Testy pro browser engine.

mod html_parser_tests;
mod css_parser_tests;
mod cascade_tests;
mod layout_tests;
mod paint_tests;
mod render_tests;
mod dom_tests;
mod devtools_panel_tests;
mod engine_test_diagnostic;
mod web_fixtures;
// L5 merge artifact: visual_snapshot.rs impl chybi lokalne (origin/master refactor
// neni komplet zmergovany). Disabled aby test suite kompilovala. Re-enable az dorazi.
// mod visual_snapshot;
