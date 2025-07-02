use crate::lexer::base::Lexer;
use crate::tokens::Keyword;

mod tokens;
mod utils;
mod specifications;
mod lexer;

fn main() {

    println!("Hello, world!");

    // Pokus o převod řetězce na enum variantu
    if let Some(keyword) = Keyword::from_str("Break") {
        println!("Rozpoznané klíčové slovo: {:?}", keyword);
    } else {
        println!("Nerozpoznané klíčové slovo!");
    }

    // Zobrazení varianty jako řetězce
    let keyword = Keyword::If;
    println!("Klíčové slovo jako string: {}", keyword.as_str());

    let lexer = Lexer::parse_file("F:\\Develop\\_Projects\\RustWebEngine\\static\\basic_test.js");

}
