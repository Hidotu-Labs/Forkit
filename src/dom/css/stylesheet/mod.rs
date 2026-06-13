mod selector;
mod parser;
mod utils;

pub use selector::{
    PseudoClass, Selector, SimpleSelector, Specificity,
    parse_selector_group, parse_selector_list, parse_simple_selector,
    parse_pseudo_class, parse_nth,
};
pub use parser::{StyleSheet, Rule, parse_declarations};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input() {
        let ss = StyleSheet::parse("");
        assert_eq!(ss.rules.len(), 0);
    }

    #[test]
    fn single_rule() {
        let ss = StyleSheet::parse("p { color: red; }");
        assert_eq!(ss.rules.len(), 1);
        let rule = &ss.rules[0];
        assert_eq!(rule.selectors, vec![Selector::Tag("p".to_string())]);
        assert_eq!(rule.declarations.len(), 1);
        assert_eq!(rule.declarations[0], ("color".to_string(), "red".to_string()));
    }

    #[test]
    fn multiple_selectors() {
        let ss = StyleSheet::parse("h1, h2 { font-weight: bold; }");
        assert_eq!(ss.rules.len(), 1);
        let rule = &ss.rules[0];
        assert_eq!(
            rule.selectors,
            vec![
                Selector::Tag("h1".to_string()),
                Selector::Tag("h2".to_string()),
            ]
        );
        assert_eq!(rule.declarations.len(), 1);
        assert_eq!(rule.declarations[0], ("font-weight".to_string(), "bold".to_string()));
    }

    #[test]
    fn syntax_error_recovery() {
        let css = "div { no-colon-here } p { color: blue; }";
        let ss = StyleSheet::parse(css);
        assert!(!ss.rules.is_empty(), "expected at least one rule");
        let p_rule = ss.rules.iter().find(|r| {
            r.selectors.iter().any(|s| matches!(s, Selector::Tag(t) if t == "p"))
        });
        assert!(p_rule.is_some(), "valid `p` rule should be present after error recovery");
        let p_rule = p_rule.unwrap();
        assert_eq!(p_rule.declarations, vec![("color".to_string(), "blue".to_string())]);
    }

    #[test]
    fn import_ignored() {
        let ss = StyleSheet::parse("@import \"style.css\"; p { color: red; }");
        assert_eq!(ss.rules.len(), 1);
        let rule = &ss.rules[0];
        assert_eq!(rule.selectors, vec![Selector::Tag("p".to_string())]);
        assert_eq!(rule.declarations[0], ("color".to_string(), "red".to_string()));
    }

    #[test]
    fn block_comment_stripped() {
        let ss = StyleSheet::parse("/* this is a comment */ p { color: green; }");
        assert_eq!(ss.rules.len(), 1);
        assert_eq!(ss.rules[0].declarations[0], ("color".to_string(), "green".to_string()));
    }

    #[test]
    fn multiple_declarations() {
        let css = "a { color: blue; text-decoration: underline; font-size: 14px; }";
        let ss = StyleSheet::parse(css);
        assert_eq!(ss.rules.len(), 1);
        let decls = &ss.rules[0].declarations;
        assert_eq!(decls.len(), 3);
        assert_eq!(decls[0], ("color".to_string(), "blue".to_string()));
        assert_eq!(decls[1], ("text-decoration".to_string(), "underline".to_string()));
        assert_eq!(decls[2], ("font-size".to_string(), "14px".to_string()));
    }

    #[test]
    fn import_url_ignored() {
        let ss = StyleSheet::parse("@import url(reset.css); h1 { font-size: 2em; }");
        assert_eq!(ss.rules.len(), 1);
        assert_eq!(ss.rules[0].selectors, vec![Selector::Tag("h1".to_string())]);
    }

    #[test]
    fn parse_selector_tag() {
        let sel = parse_selector_group("div");
        assert_eq!(sel, Some(Selector::Tag("div".to_string())));
    }

    #[test]
    fn parse_selector_class() {
        let sel = parse_selector_group(".foo");
        assert_eq!(sel, Some(Selector::Class("foo".to_string())));
    }

    #[test]
    fn parse_selector_id() {
        let sel = parse_selector_group("#bar");
        assert_eq!(sel, Some(Selector::Id("bar".to_string())));
    }

    #[test]
    fn parse_selector_universal() {
        let sel = parse_selector_group("*");
        assert_eq!(sel, Some(Selector::Universal));
    }

    #[test]
    fn parse_selector_compound() {
        let sel = parse_selector_group("div.foo");
        assert_eq!(
            sel,
            Some(Selector::Compound(vec![
                SimpleSelector::Tag("div".to_string()),
                SimpleSelector::Class("foo".to_string()),
            ]))
        );
    }

    #[test]
    fn parse_selector_descendant() {
        let sel = parse_selector_group("div p");
        assert_eq!(
            sel,
            Some(Selector::Descendant(
                Box::new(Selector::Tag("div".to_string())),
                Box::new(Selector::Tag("p".to_string())),
            ))
        );
    }

    #[test]
    fn parse_selector_child() {
        let sel = parse_selector_group("ul > li");
        assert_eq!(
            sel,
            Some(Selector::Child(
                Box::new(Selector::Tag("ul".into())),
                Box::new(Selector::Tag("li".into())),
            ))
        );
    }

    #[test]
    fn parse_selector_adjacent() {
        let sel = parse_selector_group("h1 + p");
        assert_eq!(
            sel,
            Some(Selector::AdjacentSibling(
                Box::new(Selector::Tag("h1".into())),
                Box::new(Selector::Tag("p".into())),
            ))
        );
    }

    #[test]
    fn parse_selector_general_sibling() {
        let sel = parse_selector_group("h1 ~ p");
        assert_eq!(
            sel,
            Some(Selector::GeneralSibling(
                Box::new(Selector::Tag("h1".into())),
                Box::new(Selector::Tag("p".into())),
            ))
        );
    }

    #[test]
    fn parse_selector_attr_presence() {
        let sel = parse_selector_group("[href]");
        assert_eq!(
            sel,
            Some(Selector::Compound(vec![SimpleSelector::AttrPresence("href".to_string())]))
        );
    }

    #[test]
    fn parse_selector_attr_equality() {
        let sel = parse_selector_group("[type=\"text\"]");
        assert_eq!(
            sel,
            Some(Selector::Compound(vec![
                SimpleSelector::AttrEquality("type".to_string(), "text".to_string()),
            ]))
        );
    }

    #[test]
    fn parse_selector_attr_starts_with() {
        let sel = parse_selector_group("[href^=\"https\"]");
        assert_eq!(
            sel,
            Some(Selector::Compound(vec![
                SimpleSelector::AttrStartsWith("href".to_string(), "https".to_string()),
            ]))
        );
    }

    #[test]
    fn parse_selector_attr_ends_with() {
        let sel = parse_selector_group("[href$=\".pdf\"]");
        assert_eq!(
            sel,
            Some(Selector::Compound(vec![
                SimpleSelector::AttrEndsWith("href".to_string(), ".pdf".to_string()),
            ]))
        );
    }

    #[test]
    fn parse_selector_attr_contains() {
        let sel = parse_selector_group("[href*=\"example\"]");
        assert_eq!(
            sel,
            Some(Selector::Compound(vec![
                SimpleSelector::AttrContains("href".to_string(), "example".to_string()),
            ]))
        );
    }

    #[test]
    fn parse_selector_attr_contains_word() {
        let sel = parse_selector_group("[class~=\"foo\"]");
        assert_eq!(
            sel,
            Some(Selector::Compound(vec![
                SimpleSelector::AttrContainsWord("class".to_string(), "foo".to_string()),
            ]))
        );
    }

    #[test]
    fn parse_selector_pseudo_hover() {
        let sel = parse_selector_group("a:hover");
        assert_eq!(
            sel,
            Some(Selector::Compound(vec![
                SimpleSelector::Tag("a".to_string()),
                SimpleSelector::Pseudo(PseudoClass::Hover),
            ]))
        );
    }

    #[test]
    fn parse_selector_pseudo_nth_child() {
        let sel = parse_selector_group("li:nth-child(2n+1)");
        assert_eq!(
            sel,
            Some(Selector::Compound(vec![
                SimpleSelector::Tag("li".to_string()),
                SimpleSelector::Pseudo(PseudoClass::NthChild(2, 1)),
            ]))
        );
    }

    #[test]
    fn parse_selector_pseudo_not() {
        let sel = parse_selector_group("p:not(.foo)");
        assert_eq!(
            sel,
            Some(Selector::Compound(vec![
                SimpleSelector::Tag("p".to_string()),
                SimpleSelector::Pseudo(PseudoClass::Not(
                    Box::new(SimpleSelector::Class("foo".to_string()))
                )),
            ]))
        );
    }

    #[test]
    fn parse_pseudo_element_skipped() {
        let components = parse_simple_selector("p::before");
        assert_eq!(components, vec![SimpleSelector::Tag("p".to_string())]);
    }

    #[test]
    fn parse_nth_odd() {
        assert_eq!(parse_nth("odd"), (2, 1));
    }

    #[test]
    fn parse_nth_even() {
        assert_eq!(parse_nth("even"), (2, 0));
    }

    #[test]
    fn parse_nth_integer() {
        assert_eq!(parse_nth("3"), (0, 3));
    }

    #[test]
    fn parse_nth_an_plus_b() {
        assert_eq!(parse_nth("2n+1"), (2, 1));
    }

    #[test]
    fn parse_nth_an_minus_b() {
        assert_eq!(parse_nth("3n-2"), (3, -2));
    }

    #[test]
    fn parse_nth_n_only() {
        assert_eq!(parse_nth("n"), (1, 0));
    }

    #[test]
    fn parse_nth_neg_n_plus_b() {
        assert_eq!(parse_nth("-n+3"), (-1, 3));
    }

    #[test]
    fn specificity_ordering() {
        let tag   = Specificity(0, 0, 1);
        let class = Specificity(0, 1, 0);
        let id    = Specificity(1, 0, 0);
        assert!(tag < class);
        assert!(class < id);
        assert!(tag < id);
    }

    #[test]
    fn specificity_equal() {
        assert_eq!(Specificity(1, 2, 3), Specificity(1, 2, 3));
    }

    #[test]
    fn specificity_compound_div_foo_bar() {
        let sel = parse_selector_group("div.foo#bar").expect("should parse");
        assert_eq!(sel.specificity(), Specificity(1, 1, 1));
    }

    #[test]
    fn specificity_id_only() {
        let sel = parse_selector_group("#main").expect("should parse");
        assert_eq!(sel.specificity(), Specificity(1, 0, 0));
    }

    #[test]
    fn specificity_class_only() {
        let sel = parse_selector_group(".btn").expect("should parse");
        assert_eq!(sel.specificity(), Specificity(0, 1, 0));
    }

    #[test]
    fn specificity_tag_only() {
        let sel = parse_selector_group("p").expect("should parse");
        assert_eq!(sel.specificity(), Specificity(0, 0, 1));
    }

    #[test]
    fn specificity_universal() {
        let sel = parse_selector_group("*").expect("should parse");
        assert_eq!(sel.specificity(), Specificity(0, 0, 0));
    }

    #[test]
    fn child_combinator_a_gt_b() {
        let sel = parse_selector_group("a > b").expect("should parse");
        assert_eq!(
            sel,
            Selector::Child(
                Box::new(Selector::Tag("a".to_string())),
                Box::new(Selector::Tag("b".to_string())),
            )
        );
    }

    #[test]
    fn child_combinator_specificity() {
        let sel = parse_selector_group("a > b").expect("should parse");
        assert_eq!(sel.specificity(), Specificity(0, 0, 2));
    }

    #[test]
    fn parse_simple_selector_attr_presence() {
        let parts = parse_simple_selector("[href]");
        assert_eq!(parts, vec![SimpleSelector::AttrPresence("href".to_string())]);
    }

    #[test]
    fn parse_simple_selector_attr_equality() {
        let parts = parse_simple_selector("[type=\"text\"]");
        assert_eq!(
            parts,
            vec![SimpleSelector::AttrEquality("type".to_string(), "text".to_string())]
        );
    }

    #[test]
    fn parse_selector_list_splits_on_comma() {
        let list = parse_selector_list("h1, h2, .foo");
        assert_eq!(
            list,
            vec![
                Selector::Tag("h1".to_string()),
                Selector::Tag("h2".to_string()),
                Selector::Class("foo".to_string()),
            ]
        );
    }

    #[test]
    fn parse_selector_list_empty() {
        let list = parse_selector_list("");
        assert!(list.is_empty());
    }

    #[test]
    fn attr_presence_specificity() {
        let sel = parse_selector_group("[href]").expect("should parse");
        assert_eq!(sel.specificity(), Specificity(0, 1, 0));
    }

    #[test]
    fn pseudo_class_specificity() {
        let sel = parse_selector_group("a:hover").expect("should parse");
        assert_eq!(sel.specificity(), Specificity(0, 1, 1));
    }

    #[test]
    fn general_sibling_specificity() {
        let sel = parse_selector_group("h1 ~ p").expect("should parse");
        assert_eq!(sel.specificity(), Specificity(0, 0, 2));
    }
}
