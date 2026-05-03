/// Makro generující enum s from_str / as_str metodami.
/// Každá varianta MUSÍ mít explicitní stringovou hodnotu (=> "...").
///
/// # Příklad
/// ```
/// string_enum! {
///     KeywordEnum,
///     Break => "break",
///     Let   => "let"
/// }
/// // KeywordEnum::from_str("break") == Some(KeywordEnum::Break)
/// // KeywordEnum::Break.as_str()    == "break"
/// ```
#[macro_export]
macro_rules! string_enum {
    ( $name:ident, $( $variant:ident => $s:literal ),+ $(,)? ) => {
        #[derive(Debug, PartialOrd, PartialEq, Clone)]
        pub enum $name { $( $variant ),+ }

        impl $name {
            pub fn from_str(s: &str) -> Option<Self> {
                match s { $( $s => Some(Self::$variant), )+ _ => None }
            }
            pub fn as_str(&self) -> &'static str {
                match self { $( Self::$variant => $s, )+ }
            }
        }
    };
}
