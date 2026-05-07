/// Browser engine moduly.
///
/// - dom: DOM tree (Node, Element, TextNode, Document)
/// - html_parser: parse HTML pres html5ever -> DOM
/// - css_parser: parse CSS pres cssparser -> Vec<Rule>
/// - cascade: aplikuj CSS na DOM (computed styles per element)
/// - layout: layout engine (block, inline, flex - zatim block)
/// - paint: painter generuje display list
/// - render: wgpu render loop + window management

pub mod dom;
pub mod html_parser;
pub mod css_parser;
pub mod cascade;
pub mod layout;
pub mod layout_engine;
pub mod paint;
pub mod render;
pub mod devtools_panel;
pub mod woff;
pub mod variable_fonts;

#[cfg(test)]
mod tests;
