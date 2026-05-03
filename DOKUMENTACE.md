# RustWebEngine - Dokumentace

JavaScript/ESNext interpreter naprogramovany v Rustu.

---

## Obsah

1. [Prehled architektury](#prehled-architektury)
2. [Pipeline zpracovani](#pipeline-zpracovani)
3. [Lexer](#lexer)
4. [Parser](#parser)
5. [AST](#ast)
6. [Interpreter](#interpreter)
7. [Vestavene objekty](#vestavene-objekty)
8. [Implementovane funkce JavaScriptu](#implementovane-funkce-javascriptu)
9. [Struktura projektu](#struktura-projektu)
10. [Spusteni a testy](#spusteni-a-testy)
11. [Zname omezeni](#zname-omezeni)
12. [Dalsi rozvoj](#dalsi-rozvoj)

---

## Prehled architektury

Interpreter se sklada ze tri hlavnich fazi:

```
Zdrojovy text (String)
      |
      v  [Lexer]
Vec<Token>
      |
      v  [Parser]
Program (AST)
      |
      v  [Interpreter]
JsValue (vysledek)
```

Kazda faze je nezavisla a muze byt testovana zvlast.

---

## Pipeline zpracovani

### 1. Lexer (`src/lexer/base.rs`)

Prevadi surovy text na posloupnost tokenu. Jeden pruchod zdrojovym kodem,
znak po znaku pomoci `Utf8Cursor`.

**Vystup:** `Vec<Token>` - kazdy token obsahuje:
- `kind` - druh (cislo, retezec, operator, klicove slovo, ...)
- `lexeme` - surovy text ze zdrojaku
- `line`, `column` - pozice pro chybove hlasky

### 2. Parser (`src/parser/mod.rs`)

Prevadi token stream na AST (Abstract Syntax Tree).

**Vstup:** `Vec<Token>` (trivia - whitespace a komentare - jsou ignorovany)

**Vystup:** `Program { body: Vec<Stmt>, strict: bool }`

Pouziva dva algoritmy:
- **Recursive descent** pro prikazy (if, while, for, ...)
- **Pratt parsing** pro vyrazy (spravna priorita a asociativita operatoru)

### 3. Interpreter (`src/interpreter/mod.rs`)

Prochazi AST a vyhodnocuje jednotlive uzly.

**Vstup:** `&Program`

**Vystup:** `Result<JsValue, JsError>`

---

## Lexer

### Podporovane tokeny

| Kategorie | Priklady |
|-----------|---------|
| Identifikatory | `foo`, `myVar`, `$el`, `_p`, Unicode: `promena` |
| Cisla | `42`, `3.14`, `0xFF`, `0b1010`, `0o77`, `1_000`, `42n` |
| Retezce | `"hello"`, `'world'`, s escapy: `"\n"`, `"\u{1F600}"` |
| Template literaly | `` `Hello ${name}!` `` (rozlozene na Head/Middle/Tail/NoSubstitution) |
| Regularni vyrazy | `/pattern/gi` |
| Klicova slova | `if`, `function`, `return`, `class`, `async`, ... (vsechna ECMAScript kl. slova) |
| Operatory | `+`, `===`, `=>`, `?.`, `??=`, `&&=`, `||=`, `>>>`, ... |
| Komentare | `// radkovy`, `/* blokovy */` |
| Trivia | Whitespace, Newline (LF/CR/LS/PS) |

### Template literaly

Template literal `` `Hello ${name}, ${age}!` `` je rozlozen na:
```
TemplateHead("Hello ")    <- zacatek
  [vyraz: name]
TemplateMiddle(", ")      <- prostredni cast
  [vyraz: age]
TemplateTail("!")         <- konec
```

Jednoduchy template bez vyrazu: ``  `hello` `` -> `NoSubstitutionTemplate("hello")`

### Escape sekvence v retezdich

| Syntaxe | Vysledek | Priklad |
|---------|---------|---------|
| `\n`, `\t`, `\r`, ... | Specialni znaky | `"\n"` -> newline |
| `\xHH` | Hex escape | `"\x41"` -> `"A"` |
| `\uXXXX` | Unicode 4 cislice | `"A"` -> `"A"` |
| `\u{XXXXX}` | Unicode braces | `"\u{1F600}"` -> emoji |
| `\0` - `\7` | Oktalova | `"\101"` -> `"A"` |

---

## Parser

### Priorita operatoru (od nejnizsi)

| Uroven | Operatory | Popis |
|--------|-----------|-------|
| 1 | `=`, `+=`, `-=`, `&&=`, `??=`, ... | Prirazeni (pravo-asociativni) |
| 2 | `? :` | Ternary |
| 3 | `\|\|`, `??` | Logicke NEBO, Nullish |
| 4 | `&&` | Logicke A |
| 5 | `\|` | Bitove NEBO |
| 6 | `^` | Bitove XOR |
| 7 | `&` | Bitove A |
| 8 | `==`, `!=`, `===`, `!==` | Rovnost |
| 9 | `<`, `>`, `<=`, `>=`, `in`, `instanceof` | Porovnani |
| 10 | `<<`, `>>`, `>>>` | Bitove posuvy |
| 11 | `+`, `-` | Soucet |
| 12 | `*`, `/`, `%` | Soucin |
| 13 | `**` | Mocnina (pravo-asociativni) |
| 14 | `++x`, `--x`, `-x`, `!x`, `typeof x`, ... | Unarni prefix |
| 15 | `x++`, `x--` | Unarni postfix |
| 16 | `f()`, `obj.prop`, `obj[key]`, `?.` | Volani a member access |

### Podporovane prikazy

```javascript
// Deklarace
var x;
let y = 5;
const PI = 3.14;

// Vetvenei
if (cond) { ... } else { ... }

// Cykly
while (cond) { ... }
do { ... } while (cond);
for (let i = 0; i < 10; i++) { ... }
for (const key in obj) { ... }
for (const val of arr) { ... }

// Funkce
function add(a, b) { return a + b; }

// Rizeni toku
return value;
break label;
continue;
throw new Error("msg");
try { ... } catch (e) { ... } finally { ... }

// Oznaceny prikaz
outer: for (...) {
    break outer;
}
```

### Podporovane vyrazy

```javascript
// Literaly
42, 3.14, "text", `template ${expr}`, true, false, null, undefined
[1, 2, 3], { a: 1, b: 2 }

// Operatory
a + b, a === b, a ?? b, a?.b, a?.[0], foo?.()

// Arrow funkce
x => x * 2
(a, b = 0, ...rest) => a + b
() => { return 42; }

// Vyrazy funkci
const f = function(x) { return x; }

// Volani
foo(1, 2, ...args)
obj.method()
new Constructor(args)
```

---

## AST

Kompletni definice je v `src/ast.rs`. Hlavni typy:

### `Program`

Koren AST. Obsahuje `body: Vec<Stmt>`.

### `Stmt` (prikaz)

Prikazy se vykonavaji pro svuj efekt (neprimy return hodnoty).
Varianty: `Expr`, `Block`, `Var`, `Function`, `If`, `While`, `For`, `ForIn`, `ForOf`,
`Return`, `Throw`, `Try`, `Break`, `Continue`, `Labeled`, `Empty`.

### `Expr` (vyraz)

Vyrazy se vyhodnocuji a vraci `JsValue`.
Varianty: `Number`, `Str`, `Bool`, `Null`, `Undefined`, `Regex`, `Template`,
`Ident`, `Array`, `Object`, `Unary`, `Binary`, `Logical`, `Ternary`, `Assign`,
`Call`, `New`, `Member`, `Function`, `Arrow`, `Spread`, `Sequence`.

### `Param` (parametr funkce)

```rust
pub struct Param {
    pub name: String,
    pub default: Option<Box<Expr>>,  // (x = 42)
    pub rest: bool,                   // ...args
}
```

---

## Interpreter

### JsValue (runtime hodnoty)

| Varianta | JS typ | Priklad |
|----------|--------|---------|
| `Undefined` | undefined | `undefined` |
| `Null` | null | `null` |
| `Bool(bool)` | boolean | `true`, `false` |
| `Number(f64)` | number | `42`, `3.14`, `NaN`, `Infinity` |
| `Str(String)` | string | `"hello"` |
| `Object(Rc<RefCell<JsObject>>)` | object | `{ x: 1 }` |
| `Array(Rc<RefCell<Vec<JsValue>>>)` | object (Array) | `[1, 2, 3]` |
| `Function(JsFunc)` | function | `x => x` |

`Object` a `Array` jsou sdilene pres `Rc<RefCell<>>` - umoznuje to:
- Closury referujici na stejny objekt
- Mutaci objektu pres vice referencii
- Bez garbage collectoru (Rust ownership + Rc pocitani referenci)

### Environment (scopes)

Retezec scopes implementuje lexikalni scopovani:

```
global scope
  { Math, console, parseInt, ... }
  |
  function scope (pri volani funkce)
  { arguments, this, param1, param2, ... }
    |
    block scope (pri vstupu do {})
    { x, y, ... }
```

`var` deklarace jdou do globalniho scopu (hoisting).
`let`/`const` deklarace jsou v aktualnim scopu.

### Rizeni toku pomoci Signal

Prikazy `return`, `break`, `continue` jsou implementovany pomoci `Signal` enum,
ktery se propaguje nahoru pres `exec_stmt` az na misto kde se zachyti:
- `return` -> zachyti se v `call_function`
- `break`/`continue` -> zachyti se v cyklovych prikzich (`While`, `For`, ...)

### Funkce

```javascript
// User funkce - ulozeji AST + uzavreny scope
function f(a, b = 0, ...rest) {
    return a + b;
}

// Native funkce - Rust closure
// Implementovany jako JsFunc::Native(jmeno, Rc<dyn Fn(Vec<JsValue>) -> Result<JsValue, String>>)
```

---

## Vestavene objekty

### `Math`

```javascript
Math.PI          // 3.141592...
Math.E           // 2.718281...
Math.sqrt(x)     // odmocnina
Math.abs(x)      // absolutni hodnota
Math.floor(x)    // zaokrouhleni dolu
Math.ceil(x)     // zaokrouhleni nahoru
Math.round(x)    // zaokrouhleni
Math.sin(x)      // sinus (radiany)
Math.cos(x)      // kosinus
Math.log(x)      // prirozeny logaritmus
Math.max(a, b)   // vetsi z hodnot
Math.min(a, b)   // mensi z hodnot
Math.pow(b, e)   // mocnina
Math.random()    // pseudonahodne cislo [0, 1)
```

### `console`

```javascript
console.log(...)     // standardni vystup
console.error(...)   // chybovy vystup (stderr)
console.warn(...)    // varovani (stderr)
```

### `Object`

```javascript
Object.keys(obj)              // pole klicu
Object.values(obj)            // pole hodnot
Object.entries(obj)           // pole [klic, hodnota] paru
Object.assign(target, source) // skopirovani vlastnosti
Object.freeze(obj)            // "zmrazeni" (zatim bez vynuceni)
Object.create(proto)          // novy objekt (proto se ignoruje)
Object.fromEntries(entries)   // objekt z pole [klic, hodnota] paru
```

### `Array`

```javascript
Array.isArray(x)    // true kdyz je x pole
Array.from(x)       // kopie pole nebo pole znaku z retezce
Array(n)            // pole n prvku (undefined)
Array(a, b, c)      // pole danych prvku
```

### Globalni funkce

```javascript
parseInt("42")          // cislo z retezce
parseFloat("3.14")      // desetinne cislo z retezce
isNaN(x)                // je x NaN?
isFinite(x)             // je x konecne cislo?
String(x)               // prevod na retezec
Number(x)               // prevod na cislo
Boolean(x)              // prevod na boolean
```

---

## Implementovane funkce JavaScriptu

### Array metody

| Metoda | Popis |
|--------|-------|
| `push(...items)` | Prida prvky na konec, vrati novou delku |
| `pop()` | Odebere a vrati posledni prvek |
| `shift()` | Odebere a vrati prvni prvek |
| `unshift(...items)` | Prida prvky na zacatek |
| `reverse()` | Obraci pole na miste |
| `sort(fn?)` | Razeni (volitelne s porovnavaci funkci) |
| `join(sep)` | Spoji prvky do retezce |
| `includes(val)` | Obsahuje pole danou hodnotu? |
| `indexOf(val)` | Prvni index hodnoty (-1 kdyz neni) |
| `lastIndexOf(val)` | Posledni index hodnoty |
| `slice(start, end?)` | Vybere cast pole (novy pole) |
| `concat(...arrays)` | Spoji pole dohromady |
| `flat(depth?)` | Linearizuje vnorena pole |
| `fill(val, start?, end?)` | Vyplni cast pole hodnotou |
| `splice(start, count, ...items)` | Odstrani/vlozi prvky na miste |
| `at(index)` | Prvek na indexu (zaporny = od konce) |
| `forEach(fn)` | Projde vsechny prvky (zadny return) |
| `map(fn)` | Transformuje kazdy prvek (novy pole) |
| `filter(fn)` | Vybere prvky kde fn vraci true (novy pole) |
| `reduce(fn, init)` | Akumuluje hodnotu pres vsechny prvky |
| `reduceRight(fn, init)` | Jako reduce, ale zprava |
| `find(fn)` | Prvni prvek kde fn vraci true |
| `findIndex(fn)` | Index prvniho prvku kde fn vraci true |
| `every(fn)` | Vraci true kdyz fn vraci true pro vsechny prvky |
| `some(fn)` | Vraci true kdyz fn vraci true pro aspon jeden prvek |
| `flatMap(fn)` | map + flat(1) |
| `toString()` | join(",") |

### String metody

| Metoda | Popis |
|--------|-------|
| `split(sep)` | Rozdeli retezec na pole |
| `slice(start, end?)` | Vybere cast retezce |
| `substring(start, end?)` | Vybere cast retezce (alternativa k slice) |
| `indexOf(str)` | Prvni pozice podretezce (-1 kdyz neni) |
| `lastIndexOf(str)` | Posledni pozice podretezce |
| `includes(str)` | Obsahuje retezec dany podretezec? |
| `startsWith(str)` | Zacina retezec danym prefixem? |
| `endsWith(str)` | Konci retezec danym sufixem? |
| `toLowerCase()` | Prevede na mala pismena |
| `toUpperCase()` | Prevede na velka pismena |
| `trim()` | Odebere whitespace na zacatku a konci |
| `trimStart()` | Odebere whitespace na zacatku |
| `trimEnd()` | Odebere whitespace na konci |
| `charAt(i)` | Znak na pozici i |
| `charCodeAt(i)` | Unicode hodnota znaku na pozici i |
| `at(i)` | Znak na pozici i (zaporny = od konce) |
| `padStart(len, fill)` | Doplni retezec zleva na danou delku |
| `padEnd(len, fill)` | Doplni retezec zprava na danou delku |
| `repeat(n)` | Opakuje retezec n-krat |
| `replace(from, to)` | Nahradi prvni vyskytu podretezce |
| `replaceAll(from, to)` | Nahradi vsechny vyskyty |
| `toString()`, `valueOf()` | Vrati retezec samotny |

---

## Struktura projektu

```
src/
  main.rs                  - vstupni bod
  ast.rs                   - AST typy (Expr, Stmt, Program, Param, ...)
  tokens.rs                - tokeny (TokenKind, Token, KeywordEnum, OperatorEnum)
  
  lexer/
    mod.rs                 - export lexer modulu
    base.rs                - hlavni lexer (Lexer struct, lex() smycka)
    string.rs              - lexer pro retezce a escape sekvence
    numeric.rs             - lexer pro ciselne literaly
    identifier.rs          - lexer pro identifikatory
    regex.rs               - lexer pro regularni vyrazy
    debug.rs               - pomocne debug funkce
    tests/
      string.rs            - testy pro retezce
      numeric.rs           - testy pro cisla
      regex.rs             - testy pro regularni vyrazy

  parser/
    mod.rs                 - cely parser (Pratt + recursive descent)

  interpreter/
    mod.rs                 - cely interpreter (JsValue, Environment, Interpreter)

  specifications/
    mod.rs                 - export specifikaci
    lexer_errors.rs        - definice chyb lexeru
    number_literal.rs      - specifikace ciselnych literalu

  utils/
    mod.rs                 - export utils
    utf8_cursor.rs         - char-po-char Unicode reader
    string_utils.rs        - pomocne funkce pro retezce
    macros/
      mod.rs               - export maker
      string_enum.rs       - makro string_enum!
```

---

## Spusteni a testy

### Build

```bash
cargo build
```

### Testy (160 unit testu)

```bash
cargo test
```

### Spustit konkretni test

```bash
cargo test interpreter::tests::array_map
```

### Generovat dokumentaci

```bash
cargo doc --open
```

---

## Zname omezeni

### Nepodporovano (zatim)

- **Destructuring**: `const [a, b] = arr;` nebo `const { x, y } = obj;`
- **Tridy**: `class Foo extends Bar { ... }`
- **async/await**: asynchronni kod
- **Generator funkce**: `function* gen() { yield 1; }`
- **Moduly**: `import`/`export`
- **Prototype chain**: `obj.__proto__`, `Object.getPrototypeOf()`
- **Symbol, WeakMap, WeakSet, Map, Set**: specialni kolekce
- **Regex**: regularni vyrazy se tokenizuji, ale nevyhodncuji
- **BigInt aritmetika**: BigInt literaly jsou parsovany, hodnota je NaN
- **Getter/setter**: `get prop() { }` v tridach a objektech
- **Computed method names**: `{ [expr]() { } }`
- **Object.freeze** realny immutability

### Castecna podpora

- **`new`**: parsovano, ale nema prototype-based OOP
- **`this`**: predavano do metod jako parametr, ale `globalThis` a `window` neni
- **`arguments`**: dostupny v user funkci jako pole hodnot
- **Regex literaly**: tokenizovany, ale `RegExp.exec()` atd. neni implementovano

---

## Dalsi rozvoj

### Batch 2: Destructuring

```javascript
const [a, b, ...rest] = [1, 2, 3, 4];
const { x, y: renamed, z = 10 } = obj;
function f({ name, age = 0 }) { }
```

### Batch 3: Tridy

```javascript
class Animal {
    constructor(name) { this.name = name; }
    speak() { return `${this.name} says something`; }
}
class Dog extends Animal {
    speak() { return `${this.name} barks`; }
}
```

### Batch 4: Prototype chain

- `Object.getPrototypeOf()`
- `Object.setPrototypeOf()`
- `instanceof` s dedicnosti
- `hasOwnProperty()`

### Batch 5: Iteratory a generatory

```javascript
function* range(n) {
    for (let i = 0; i < n; i++) yield i;
}
for (const x of range(5)) console.log(x);
```

### Batch 6: TypeScript (transpile + full type checker)

Planovana implementace ve dvou krocich:

**Krok 1 - Transpile-only** (jako esbuild/Babel):
- Parser precte TS syntaxi a zahodi type anotace
- `:` za identifikatorem = parsuj a zahod typ
- `interface`, `type` deklarace = ignorovany
- `x as string` = eval pouze `x`
- `<T>` generics = zahozeny
- `enum Direction { Up, Down }` = prevedeno na JS objekt (jedine s runtime vyznamem)
- `readonly`, `public`, `private`, `protected` = zahozeny modifier

**Krok 2 - Full type checker** (jako tsc):
- Typova inference pro vsechny vyrazy
- Kontrola prirazeni (structural typing)
- Genericke typy s omezenim (`T extends Foo`)
- Conditional types, mapped types, template literal types
- Chybove hlasky s pozici (jako `tsc --noEmit`)

Poznamka: TypeScript typovy system je jeden z nejslozitejsich existujicich.
Krok 2 je mesice/roky prace - bude implementovan az bude interpreter kompletni.

### Mozne vylepseni interpretu

- **Garbage collector** misto Rc (pro cyklicke reference)
- **Kompilace do bytekodu** pro rychlejsi vyhodnocovani
- **Source mapy** pro lepsi chybove hlasky
- **Strict mode** vynucovani
- **WeakRef / FinalizationRegistry**
