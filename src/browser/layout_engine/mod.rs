/// Vlastni layout engine - flex / grid.
///
/// Inspirovano `taffy` crate (MIT licence, https://github.com/DioxusLabs/taffy).
/// Zacatek byl wrapper pres taffy, postupne nahrazujeme vlastni implementaci aby
/// sme meli plnou kontrolu nad layout chovanim a mohli pridat custom features
/// (subgrid, real shape-outside, atd.) ktere taffy nepodporuje.
///
/// Kompletni flex spec je velky - implementujeme JEN co realne v rendrovanych
/// strankach pouzivame: row/column direction, wrap, justify-content,
/// align-items, gap, basic flex-grow/shrink.

pub mod flex;
pub mod grid;

pub use flex::{layout_flex, FlexDirection, FlexWrap, JustifyContent, AlignItems};
pub use grid::layout_grid;
