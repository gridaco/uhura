//! Executable contracts for checked UI elements.
//!
//! Syntax parsing intentionally remains element-name agnostic. This module is
//! the semantic boundary that owns the current elements, attributes,
//! constraints, and events. Renderer mechanics stay outside the checker, but
//! they can be compared against this finite catalogue.

mod elements;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Catalog;

pub(crate) const fn current() -> Catalog {
    Catalog
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Availability {
    Native,
    StandardImport,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ContentModel {
    Children,
    Void,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BrowserRealization {
    /// Native HTML needs no Uhura-specific browser lifecycle adapter.
    NativeHtml,
    /// The browser renderer must publish one adapter with this exact ID.
    PrimitiveAdapter,
    /// Core projection lowers this standard extension before browser rendering.
    CoreExtension,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AttributeKind {
    Text,
    Bool,
    ExactNumeric,
    /// An exact, normalized scalar in the inclusive range `0..1`.
    Ratio,
    StaticToken(&'static [&'static str]),
    /// A framework-owned value whose exact type is checked by its projection
    /// contract rather than the primitive catalogue.
    CheckedExpression,
    /// Any scalar or nominal scalar-key value.
    Key,
}

impl AttributeKind {
    pub(crate) fn requires_expression(self) -> bool {
        matches!(
            self,
            Self::Bool | Self::ExactNumeric | Self::Ratio | Self::CheckedExpression | Self::Key
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct AttributeSpec {
    pub(crate) name: &'static str,
    pub(crate) kind: AttributeKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EventPayload {
    Unit,
    TextField(&'static str),
    BoundaryNumberField(&'static str),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EventCondition {
    Always,
    TextInput,
    NumberInput,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct EventSpec {
    pub(crate) name: &'static str,
    pub(crate) payload: EventPayload,
    pub(crate) condition: EventCondition,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Constraint {
    ExactlyOneAttribute(&'static [&'static str]),
    Controlled {
        attribute: &'static str,
        event: &'static str,
    },
    AccessibleName {
        attributes: &'static [&'static str],
    },
    NoInteractiveDescendants,
    AtLeastOneEvent(&'static [&'static str]),
    /// A literal list must expose a neutral direct node for the browser to
    /// own as `listitem`; semantic or interactive children stay nested inside.
    NeutralListItems {
        element: &'static str,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ElementSpec {
    pub(crate) name: &'static str,
    pub(crate) availability: Availability,
    pub(crate) content: ContentModel,
    pub(crate) browser_realization: BrowserRealization,
    pub(crate) interactive: bool,
    pub(crate) attributes: &'static [AttributeSpec],
    pub(crate) required_attributes: &'static [&'static str],
    pub(crate) events: &'static [EventSpec],
    pub(crate) constraints: &'static [Constraint],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ElementContext {
    pub(crate) static_number_input: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EventContract {
    Admitted(EventPayload),
    RequiresTextInput,
    RequiresNumberInput,
    Unknown,
}

impl Catalog {
    pub(crate) fn element(self, name: &str) -> Option<&'static ElementSpec> {
        elements::element(name)
    }

    pub(crate) fn attribute(
        self,
        element: &ElementSpec,
        attribute: &str,
        context: ElementContext,
    ) -> Option<AttributeKind> {
        if attribute == "class" {
            return Some(AttributeKind::Text);
        }
        if element.name == "input" && attribute == "value" && context.static_number_input {
            return Some(AttributeKind::ExactNumeric);
        }
        element
            .attributes
            .iter()
            .find_map(|spec| (spec.name == attribute).then_some(spec.kind))
    }

    pub(crate) fn event(
        self,
        element: &ElementSpec,
        event: &str,
        context: ElementContext,
    ) -> EventContract {
        let Some(spec) = element.events.iter().find(|spec| spec.name == event) else {
            return EventContract::Unknown;
        };
        match spec.condition {
            EventCondition::Always => EventContract::Admitted(spec.payload),
            EventCondition::TextInput if !context.static_number_input => {
                EventContract::Admitted(spec.payload)
            }
            EventCondition::TextInput => EventContract::RequiresTextInput,
            EventCondition::NumberInput if context.static_number_input => {
                EventContract::Admitted(spec.payload)
            }
            EventCondition::NumberInput => EventContract::RequiresNumberInput,
        }
    }

    pub(crate) fn is_interactive(self, name: &str) -> bool {
        self.element(name)
            .is_some_and(|element| element.interactive)
    }
}

/// Exact browser adapter IDs required by the active Uhura catalogue.
///
/// Only this cross-language parity seam is public; the full constraint model
/// remains an implementation detail of the checker.
pub fn primitive_adapter_ids() -> Vec<&'static str> {
    let mut ids = elements::ELEMENTS
        .iter()
        .filter_map(|element| {
            (element.browser_realization == BrowserRealization::PrimitiveAdapter)
                .then_some(element.name)
        })
        .collect::<Vec<_>>();
    ids.sort_unstable();
    ids
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    fn assert_closed(elements: &[ElementSpec]) {
        let mut element_names = BTreeSet::new();
        for element in elements {
            assert!(
                element_names.insert(element.name),
                "duplicate UI element `{}`",
                element.name
            );

            let mut attributes = BTreeSet::from(["class"]);
            for attribute in element.attributes {
                assert!(
                    attributes.insert(attribute.name),
                    "duplicate attribute `{}` on `<{}>`",
                    attribute.name,
                    element.name
                );
            }
            for required in element.required_attributes {
                assert!(
                    attributes.contains(required),
                    "required attribute `{required}` is not declared on `<{}>`",
                    element.name
                );
            }

            let mut events = BTreeSet::new();
            for event in element.events {
                assert!(
                    events.insert(event.name),
                    "duplicate event `{}` on `<{}>`",
                    event.name,
                    element.name
                );
            }

            for constraint in element.constraints {
                match constraint {
                    Constraint::ExactlyOneAttribute(names) => {
                        assert!(names.len() > 1);
                        for name in *names {
                            assert!(attributes.contains(name));
                        }
                    }
                    Constraint::Controlled { attribute, event } => {
                        assert!(attributes.contains(attribute));
                        assert!(events.contains(event));
                    }
                    Constraint::AccessibleName {
                        attributes: name_attributes,
                    } => {
                        for attribute in *name_attributes {
                            assert!(attributes.contains(attribute));
                        }
                    }
                    Constraint::NoInteractiveDescendants => {}
                    Constraint::AtLeastOneEvent(names) => {
                        assert!(!names.is_empty());
                        for name in *names {
                            assert!(events.contains(name));
                        }
                    }
                    Constraint::NeutralListItems {
                        element: item_element,
                    } => {
                        assert_eq!(element.name, "view");
                        assert!(attributes.contains("role"));
                        assert!(elements.iter().any(|candidate| {
                            candidate.name == *item_element && !candidate.interactive
                        }));
                    }
                }
            }
        }
    }

    #[test]
    fn catalogue_is_a_closed_table() {
        assert_closed(elements::ELEMENTS);
    }

    #[test]
    fn scroll_position_is_a_checked_ratio() {
        let catalog = current();
        let scroll = catalog.element("scroll").expect("scroll contract");
        let context = ElementContext {
            static_number_input: false,
        };
        assert_eq!(
            catalog.attribute(scroll, "position", context),
            Some(AttributeKind::Ratio)
        );
    }

    #[test]
    fn view_roles_exclude_incomplete_tablist_semantics() {
        let catalog = current();
        let view = catalog.element("view").expect("view contract");
        let context = ElementContext {
            static_number_input: false,
        };
        assert_eq!(
            catalog.attribute(view, "role", context),
            Some(AttributeKind::StaticToken(&["none", "list", "navigation"]))
        );
    }

    #[test]
    fn catalogue_carries_authoring_constraints() {
        let catalog = current();
        let current_button = catalog.element("button").expect("button contract");
        assert!(matches!(
            current_button.constraints.first(),
            Some(Constraint::AccessibleName { .. })
        ));

        let current_textfield = catalog.element("textfield").expect("textfield contract");
        assert_eq!(
            current_textfield.constraints,
            &[Constraint::Controlled {
                attribute: "value",
                event: "change"
            }]
        );
    }

    #[test]
    fn event_lookup_is_closed() {
        let catalog = current();
        let scroll = catalog.element("scroll").expect("scroll");
        let context = ElementContext {
            static_number_input: false,
        };

        assert_eq!(
            catalog.event(scroll, "near-end", context),
            EventContract::Admitted(EventPayload::Unit)
        );
        assert_eq!(
            catalog.event(scroll, "future-event", context),
            EventContract::Unknown
        );
    }
}
