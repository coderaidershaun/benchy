//! Mutator tests: what each operator changes in each language, what it leaves alone, and a sample that one seed fixes.

use super::*;

fn mutants(code: &str, language: Language) -> Vec<(Operator, String, String)> {
    candidates(Path::new("f"), code, language, |_| true)
        .into_iter()
        .map(|m| (m.operator, m.original, m.replacement))
        .collect()
}

fn pairs(code: &str, language: Language) -> Vec<String> {
    mutants(code, language)
        .into_iter()
        .map(|(_, from, to)| format!("{from}>{to}"))
        .collect()
}

#[test]
fn comparisons_flip_and_shift_their_boundary() {
    assert_eq!(
        pairs("    let x = a <= b;", Language::Rust),
        ["<=>>=", "<=><"]
    );
    assert_eq!(pairs("return a == b", Language::Go), ["==>!="]);
}

#[test]
fn arithmetic_swaps_also_in_compound_assignments() {
    assert_eq!(
        pairs("total += qty * 2;", Language::CSharp)[..2],
        ["+=>-=", "*>/"]
    );
}

#[test]
fn generics_arrows_pointers_and_unary_signs_are_left_alone() {
    let rust = "fn f(x: Vec<u8>) -> Option<&'a T> { let y = -x; let z = *p; }";
    let go = "func f(p *Book, c chan<- int) {";

    assert!(mutants(rust, Language::Rust).is_empty());
    assert!(mutants(go, Language::Go).is_empty());
}

#[test]
fn conditions_are_negated_in_the_syntax_of_each_language() {
    let negated = |code: &str, language| {
        mutants(code, language)
            .into_iter()
            .filter(|(op, ..)| *op == Operator::NegateCondition)
            .map(|(_, from, to)| format!("{from}>{to}"))
            .collect::<Vec<_>>()
    };

    assert_eq!(
        negated("    if self.is_empty() {", Language::Rust),
        ["self.is_empty()>!(self.is_empty())"]
    );
    assert_eq!(
        negated("\t} else if v, ok := m[k]; ok {", Language::Go),
        ["ok>!(ok)"]
    );
    assert_eq!(
        negated("        if (book.Bids.Count(x) > 0)", Language::CSharp),
        ["book.Bids.Count(x) > 0>!(book.Bids.Count(x) > 0)"]
    );
    assert!(negated("    if let Some(x) = y {", Language::Rust).is_empty());
    assert!(negated("    let a = if b { 1 } else { 2 };", Language::Rust).is_empty());
}

#[test]
fn integer_and_boolean_constants_change_but_names_and_fields_do_not() {
    assert_eq!(
        pairs(
            "let x = f(0, 1u64, 255, true) + v2.0 + 1.5 + 0xff;",
            Language::Rust
        )
        .into_iter()
        .filter(|p| !p.starts_with('+'))
        .collect::<Vec<_>>(),
        ["0>1", "1>0", "255>256", "true>false"]
    );
}

#[test]
fn attribute_lines_and_ineligible_lines_have_no_mutants() {
    let code = "#[derive(Clone)]\nlet a = 1;\nlet b = 2;\n";

    let found = candidates(Path::new("f.rs"), code, Language::Rust, |line| line != 3);

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].line, 2);
}

#[test]
fn a_mutant_applies_at_its_place_even_after_wide_characters() {
    let text = "// é\nlet x = a < b; // ü < v\n";
    let code = "    \nlet x = a < b;         \n";
    let found = candidates(Path::new("f.rs"), code, Language::Rust, |_| true);

    let mutated = found[0].apply(text).expect("the original is in place");

    assert_eq!(mutated, "// é\nlet x = a > b; // ü < v\n");
    assert_eq!(found[0].apply("something else"), None);
}

fn many_candidates() -> Vec<Mutant> {
    let code: String = (0..400).map(|i| format!("let v{i} = a + {i};\n")).collect();
    candidates(Path::new("lib.rs"), &code, Language::Rust, |_| true)
}

#[test]
fn the_sample_is_the_same_for_one_seed_and_differs_for_another() {
    let all = many_candidates();

    let first = sample(&all, 150, SEED);
    let again = sample(&all, 150, SEED);
    let other = sample(&all, 150, SEED + 1);

    assert_eq!(first.len(), 150);
    assert_eq!(first, again);
    assert_ne!(first, other);
}

#[test]
fn the_sample_keeps_source_order_and_takes_everything_when_there_are_few() {
    let all = many_candidates();

    let chosen = sample(&all, 150, SEED);
    let few = sample(&all[..10], 150, SEED);

    assert!(
        chosen
            .windows(2)
            .all(|w| (w[0].line, w[0].column) < (w[1].line, w[1].column))
    );
    assert_eq!(few, all[..10]);
}
