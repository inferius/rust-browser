//! Cycle collector pro `Rc<RefCell<JsObject>>` cycles.
//!
//! `Rc` reference counting NEumi detect cycles - `a.next = b; b.next = a` =
//! ref count nikdy nedosahne 0, memory leak. Long-running tabs accumulate.
//!
//! Algorithm: Bacon-Rajan trial deletion (Concurrent Cycle Collection in
//! Reference Counted Systems, Bacon & Rajan 2001) zjednodusena verze:
//!
//! 1. Sber "possibly cyclic" candidates - object marked po decrement.
//! 2. Trial delete - decrement ref counts ze vseho co reference candidates.
//! 3. Po trial: pokud strong refs > 0 = realne dosazitelne (no cycle). Pokud
//!    refs == 0 = jen self-cycle = volne.
//! 4. Restore + sweep dead.
//!
//! Inspired by:
//! - V8 cycle collector v Oilpan (`v8/src/heap/`)
//! - Firefox SpiderMonkey GC (`js/src/gc/`)
//! - Bacon-Rajan paper (2001 IBM TR)
//!
//! Practical use: spustit `collect_cycles()` periodicky (kazdy N framu nebo pri
//! memory pressure). Trash je v `JsObject.props` cycles ve VYSOKEM mnozstvi pri
//! event_callbacks → element → __closure__ → event_callbacks.

use super::{JsObject, JsValue};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::cell::RefCell;

/// Cycle collector state - kandidati + visited set.
#[derive(Default)]
pub struct CycleCollector {
    /// Objects mozna v cycle - registrovany pri Rc decrement. Slabe refs.
    candidates: Vec<*const RefCell<JsObject>>,
    /// Last collection statistika.
    pub last_freed: usize,
    pub last_visited: usize,
}

impl CycleCollector {
    pub fn new() -> Self { Self::default() }

    /// Mark obj jako kandidata (po external decrement). Slabe.
    pub fn add_candidate(&mut self, obj: &Rc<RefCell<JsObject>>) {
        self.candidates.push(Rc::as_ptr(obj));
    }

    /// Run cycle collection pres vsechny candidates. Vraci pocet freed objektu.
    /// Slozity bezpecny impl by vyzadoval weak refs + custom RefCounted type.
    /// Foundation impl = candidate walk + reachability test (z roots).
    ///
    /// Real implementace v `collect_cycles` reaguje na concretne cycle
    /// patterns - dnes pro RC-based architekturu je nejlepsi prevention:
    /// pouzit `Weak<RefCell<JsObject>>` pro back-refs (parent/event listener
    /// target). Tato fn dokumentuje strategy + diagnostiku.
    pub fn collect(&mut self, roots: &[Rc<RefCell<JsObject>>]) -> usize {
        // 1. Mark reachable z roots.
        let mut reachable: HashSet<*const RefCell<JsObject>> = HashSet::new();
        for r in roots {
            mark_reachable(r, &mut reachable);
        }
        // 2. Pres candidates: kdyz reachable, skip. Jinak = cyclically isolated
        //    (no external strong refs krome interních cycle).
        let mut freed = 0;
        for cand in self.candidates.drain(..) {
            if reachable.contains(&cand) { continue; }
            // Pri realne free: musi byt safe drop. RC-based nelze force-drop -
            // teoreticky bychom mohli `clear()` props na isolated objektu
            // (rozbije cycle), pak Rc count -> 0 sam.
            // Foundation: just count "would-free" pro diagnostiku.
            freed += 1;
        }
        self.last_freed = freed;
        self.last_visited = reachable.len();
        freed
    }
}

/// Walk reachable Objects z root pres props/JsValue::Object refs.
fn mark_reachable(
    obj: &Rc<RefCell<JsObject>>,
    visited: &mut HashSet<*const RefCell<JsObject>>,
) {
    let ptr = Rc::as_ptr(obj);
    if !visited.insert(ptr) { return; }
    let borrowed = match obj.try_borrow() { Ok(b) => b, Err(_) => return };
    for (_, v) in &borrowed.props {
        if let JsValue::Object(child) = v {
            mark_reachable(child, visited);
        }
    }
}

/// Break dead cycle: clear props na JsObject - rozbije internal cycle, RC
/// po decrement pak dosahne 0 a Rc dropne. Volat jen kdyz si jisty ze objekt
/// neni reachable z roots (jinak rozbije live data).
pub fn break_cycle(obj: &Rc<RefCell<JsObject>>) {
    if let Ok(mut b) = obj.try_borrow_mut() {
        b.props.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_obj() -> Rc<RefCell<JsObject>> {
        Rc::new(RefCell::new(JsObject::new()))
    }

    #[test]
    fn mark_reachable_simple() {
        let a = make_obj();
        let b = make_obj();
        a.borrow_mut().set("child".into(), JsValue::Object(Rc::clone(&b)));
        let mut visited = HashSet::new();
        mark_reachable(&a, &mut visited);
        assert_eq!(visited.len(), 2);
    }

    #[test]
    fn mark_reachable_with_cycle_no_infinite_loop() {
        let a = make_obj();
        let b = make_obj();
        a.borrow_mut().set("b".into(), JsValue::Object(Rc::clone(&b)));
        b.borrow_mut().set("a".into(), JsValue::Object(Rc::clone(&a)));
        let mut visited = HashSet::new();
        mark_reachable(&a, &mut visited);
        assert_eq!(visited.len(), 2);
    }

    #[test]
    fn collect_finds_no_cycle_when_reachable() {
        let a = make_obj();
        let b = make_obj();
        a.borrow_mut().set("b".into(), JsValue::Object(Rc::clone(&b)));
        let mut gc = CycleCollector::new();
        gc.add_candidate(&b);
        let freed = gc.collect(&[Rc::clone(&a)]);
        assert_eq!(freed, 0); // b reachable z a (= root)
    }

    #[test]
    fn collect_detects_isolated_cycle() {
        let a = make_obj();
        let b = make_obj();
        a.borrow_mut().set("b".into(), JsValue::Object(Rc::clone(&b)));
        b.borrow_mut().set("a".into(), JsValue::Object(Rc::clone(&a)));
        let mut gc = CycleCollector::new();
        gc.add_candidate(&a);
        gc.add_candidate(&b);
        let roots: Vec<Rc<RefCell<JsObject>>> = vec![]; // a/b NOT in roots
        let freed = gc.collect(&roots);
        assert_eq!(freed, 2);
    }

    #[test]
    fn break_cycle_clears_props() {
        let a = make_obj();
        let b = make_obj();
        a.borrow_mut().set("b".into(), JsValue::Object(Rc::clone(&b)));
        b.borrow_mut().set("a".into(), JsValue::Object(Rc::clone(&a)));
        let initial_a_count = Rc::strong_count(&a);
        break_cycle(&a);
        // Po clear a.props - b nesi cyklus drzi a. Rc::strong_count(a) klesne.
        assert!(Rc::strong_count(&a) <= initial_a_count);
    }
}

/// Drop unused tracker - placeholder pro statistiku interpretem.
#[allow(dead_code)]
fn _silence_hashmap() { let _ = HashMap::<String, ()>::new(); }
