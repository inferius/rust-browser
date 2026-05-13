/// Tridy - constructor, methods, static, inheritance, super, getter/setter.

use super::helpers::*;

#[test]
fn class_basic_constructor_and_method() {
    assert_eq!(as_str(run(r#"
        class Animal {
            constructor(name) { this.name = name; }
            speak() { return this.name + " makes a noise."; }
        }
        const a = new Animal("Dog");
        return a.speak();
    "#)), "Dog makes a noise.");
}

#[test]
fn class_properties_set_in_constructor() {
    assert_eq!(as_str(run(r#"
        class Person {
            constructor(name, age) {
                this.name = name;
                this.age = age;
            }
        }
        const p = new Person("Alice", 30);
        return p.name + " " + p.age;
    "#)), "Alice 30");
}

#[test]
fn class_multiple_methods() {
    assert_eq!(as_num(run(r#"
        class Counter {
            constructor() { this.count = 0; }
            inc() { this.count += 1; }
            get_count() { return this.count; }
        }
        const c = new Counter();
        c.inc(); c.inc(); c.inc();
        return c.get_count();
    "#)), 3.0);
}

#[test]
fn class_static_method() {
    assert_eq!(as_num(run(r#"
        class MathHelper {
            static add(a, b) { return a + b; }
            static multiply(a, b) { return a * b; }
        }
        return MathHelper.add(3, 4) + MathHelper.multiply(2, 5);
    "#)), 17.0);
}

#[test]
fn class_inheritance_basic() {
    assert_eq!(as_str(run(r#"
        class Animal {
            constructor(name) { this.name = name; }
            speak() { return this.name + " makes a noise."; }
        }
        class Dog extends Animal {
            constructor(name, breed) {
                super(name);
                this.breed = breed;
            }
        }
        const d = new Dog("Rex", "Labrador");
        return d.name + "/" + d.breed + "/" + d.speak();
    "#)), "Rex/Labrador/Rex makes a noise.");
}

#[test]
fn class_method_override() {
    assert_eq!(as_str(run(r#"
        class Animal {
            constructor(name) { this.name = name; }
            speak() { return this.name + " makes a noise."; }
        }
        class Dog extends Animal {
            constructor(name) { super(name); }
            speak() { return this.name + " barks."; }
        }
        const d = new Dog("Rex");
        return d.speak();
    "#)), "Rex barks.");
}

#[test]
fn class_super_method_call() {
    assert_eq!(as_str(run(r#"
        class Animal {
            constructor(name) { this.name = name; }
            speak() { return this.name + " makes a noise."; }
        }
        class Dog extends Animal {
            constructor(name) { super(name); }
            speak() { return super.speak() + " Woof!"; }
        }
        const d = new Dog("Rex");
        return d.speak();
    "#)), "Rex makes a noise. Woof!");
}

#[test]
fn class_no_constructor_auto_super() {
    assert_eq!(as_str(run(r#"
        class Animal {
            constructor(name) { this.name = name; }
            speak() { return this.name; }
        }
        class Cat extends Animal {
            purr() { return this.name + " purrs."; }
        }
        const c = new Cat("Whiskers");
        return c.speak() + " / " + c.purr();
    "#)), "Whiskers / Whiskers purrs.");
}

#[test]
fn class_instanceof() {
    assert!(as_bool(run(r#"
        class Animal {}
        class Dog extends Animal {}
        const d = new Dog();
        return d instanceof Dog;
    "#)));
}

#[test]
fn class_instanceof_parent() {
    assert!(as_bool(run(r#"
        class Animal {}
        class Dog extends Animal {}
        const d = new Dog();
        return d instanceof Animal;
    "#)));
}

#[test]
fn class_instanceof_false() {
    assert!(!as_bool(run(r#"
        class Animal {}
        class Dog extends Animal {}
        const a = new Animal();
        return a instanceof Dog;
    "#)));
}

#[test]
fn class_expression() {
    assert_eq!(as_str(run(r#"
        const Cat = class {
            constructor(name) { this.name = name; }
        };
        return new Cat("Kitty").name;
    "#)), "Kitty");
}

#[test]
fn class_getter_basic() {
    assert_eq!(as_num(run(r#"
        class Circle {
            constructor(r) { this.r = r; }
            get area() { return 3.14159 * this.r * this.r; }
        }
        const c = new Circle(5);
        return c.area;
    "#)), 3.14159 * 25.0);
}

#[test]
fn class_setter_basic() {
    assert_eq!(as_str(run(r#"
        class Person {
            constructor(name) { this._name = name; }
            get name() { return this._name; }
            set name(v) { this._name = v.trim(); }
        }
        const p = new Person("Alice");
        p.name = "  Bob  ";
        return p.name;
    "#)), "Bob");
}

#[test]
fn class_three_level_inheritance() {
    assert_eq!(as_str(run(r#"
        class A {
            constructor() { this.val = "A"; }
            who() { return "A"; }
        }
        class B extends A {
            constructor() { super(); this.val += "B"; }
            who() { return super.who() + "B"; }
        }
        class C extends B {
            constructor() { super(); this.val += "C"; }
            who() { return super.who() + "C"; }
        }
        const c = new C();
        return c.val + "/" + c.who();
    "#)), "ABC/ABC");
}

#[test]
fn class_method_uses_this() {
    assert_eq!(as_num(run(r#"
        class Rect {
            constructor(w, h) { this.w = w; this.h = h; }
            area() { return this.w * this.h; }
            perimeter() { return 2 * (this.w + this.h); }
        }
        const r = new Rect(3, 4);
        return r.area() + r.perimeter();
    "#)), 26.0);
}
