#[macro_export]
macro_rules! string_enum {
    ( $enum_name:ident, $( $variant:ident ),* ) => {
        #[derive(Debug)]
        pub enum $enum_name {
            $( $variant ),*   // Generování enum variant
        }

        impl $enum_name {
            pub fn from_str(s: &str) -> Option<Self> {
                match s {
                    $( stringify!($variant) => Some(Self::$variant), )*
                    _ => None,
                }
            }

            pub fn as_str(&self) -> &'static str {
                match self {
                    $( Self::$variant => stringify!($variant), )*
                }
            }
        }
    };
}
