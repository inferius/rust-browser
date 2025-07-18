#! /usr/bin/env node
// 1. Prohlášení proměnné
let x = 42;

// 2. Funkce
function foo(a, b) {
    return a + b;
}

// 3. Arrow funkce
const arrow = (x) => x * x;

// 4. Podmínka
if (x > 5) {
    console.log("větší");
} else {
    console.log("menší nebo rovno");
}

// 5. Pole a indexace
let arr = [1, 2, 3];
arr[0] = 10;

// 6. Objektový literál
const obj = { a: 1, b: "two" };

// 7. Template string s výrazem
const tpl = `Ahoj ${name}!`;

// 8. Template string s více výrazy
const nestedTpl = `Hello ${`world ${x}`}`;

// 9. Destructuring
const {a, b} = obj;
const [c, d] = arr;

// 10. Ternární operátor
let cond = x > 10 ? "big" : "small";

// 11. Logické operátory
let t = a && b || !c;

// 12. Čísla s exponentem
let num = 6.5e-2;

// 13. Regex literal
let regex = /ab+c/i;

// 14. Volání funkce
foo(x, arr[2]);

// 15. Mužný statement (užití let bez inicializace)
let uninit;
