//! The checked Uhura UI catalogue.
//!
//! Keep the finite semantic vocabulary here. Adding a primitive should update
//! one `ElementSpec`, its focused checker tests, and the corresponding browser
//! adapter; it should not add another element-name switch to the checker.

use super::{
    AttributeKind, AttributeSpec, Availability, BrowserRealization, Constraint, ContentModel,
    ElementSpec, EventCondition, EventPayload, EventSpec,
};

const NONE: &[AttributeSpec] = &[];
const NONE_REQUIRED: &[&str] = &[];
const NONE_EVENTS: &[EventSpec] = &[];
const NONE_CONSTRAINTS: &[Constraint] = &[];

const ARIA_LABEL: &[AttributeSpec] = &[AttributeSpec {
    name: "aria-label",
    kind: AttributeKind::Text,
}];

const FIELDSET_ATTRIBUTES: &[AttributeSpec] = &[
    AttributeSpec {
        name: "aria-label",
        kind: AttributeKind::Text,
    },
    AttributeSpec {
        name: "disabled",
        kind: AttributeKind::Bool,
    },
];

const INPUT_ATTRIBUTES: &[AttributeSpec] = &[
    AttributeSpec {
        name: "aria-label",
        kind: AttributeKind::Text,
    },
    AttributeSpec {
        name: "type",
        kind: AttributeKind::Text,
    },
    AttributeSpec {
        name: "disabled",
        kind: AttributeKind::Bool,
    },
    AttributeSpec {
        name: "min",
        kind: AttributeKind::ExactNumeric,
    },
    AttributeSpec {
        name: "max",
        kind: AttributeKind::ExactNumeric,
    },
    AttributeSpec {
        name: "value",
        kind: AttributeKind::Text,
    },
];
const INPUT_EVENTS: &[EventSpec] = &[
    EventSpec {
        name: "input",
        payload: EventPayload::TextField("value"),
        condition: EventCondition::TextInput,
    },
    EventSpec {
        name: "change",
        payload: EventPayload::BoundaryNumberField("number"),
        condition: EventCondition::NumberInput,
    },
];

const PROGRESS_ATTRIBUTES: &[AttributeSpec] = &[
    AttributeSpec {
        name: "aria-label",
        kind: AttributeKind::Text,
    },
    AttributeSpec {
        name: "value",
        kind: AttributeKind::ExactNumeric,
    },
    AttributeSpec {
        name: "max",
        kind: AttributeKind::ExactNumeric,
    },
];

const BUTTON_ATTRIBUTES: &[AttributeSpec] = &[
    AttributeSpec {
        name: "aria-label",
        kind: AttributeKind::Text,
    },
    AttributeSpec {
        name: "label",
        kind: AttributeKind::Text,
    },
    AttributeSpec {
        name: "aria-pressed",
        kind: AttributeKind::Bool,
    },
    AttributeSpec {
        name: "disabled",
        kind: AttributeKind::Bool,
    },
    AttributeSpec {
        name: "busy",
        kind: AttributeKind::Bool,
    },
    AttributeSpec {
        name: "pressed",
        kind: AttributeKind::Bool,
    },
    AttributeSpec {
        name: "current",
        kind: AttributeKind::Bool,
    },
];
const BUTTON_EVENTS: &[EventSpec] = &[EventSpec {
    name: "press",
    payload: EventPayload::Unit,
    condition: EventCondition::Always,
}];
const BUTTON_CONSTRAINTS: &[Constraint] = &[
    Constraint::AccessibleName {
        attributes: &["label", "aria-label"],
    },
    Constraint::NoInteractiveDescendants,
    Constraint::AtLeastOneEvent(&["press"]),
];

const VIEW_ATTRIBUTES: &[AttributeSpec] = &[AttributeSpec {
    name: "role",
    kind: AttributeKind::StaticToken(&["none", "list", "navigation"]),
}];
const VIEW_CONSTRAINTS: &[Constraint] = &[Constraint::NeutralListItems { element: "view" }];

const SCROLL_ATTRIBUTES: &[AttributeSpec] = &[
    AttributeSpec {
        name: "direction",
        kind: AttributeKind::StaticToken(&["vertical", "horizontal"]),
    },
    AttributeSpec {
        name: "position",
        kind: AttributeKind::Ratio,
    },
];
const SCROLL_EVENTS: &[EventSpec] = &[EventSpec {
    name: "near-end",
    payload: EventPayload::Unit,
    condition: EventCondition::Always,
}];

const PAGER_ATTRIBUTES: &[AttributeSpec] = &[
    AttributeSpec {
        name: "indicator",
        kind: AttributeKind::StaticToken(&["none", "dots"]),
    },
    AttributeSpec {
        name: "label",
        kind: AttributeKind::Text,
    },
];
const PAGER_REQUIRED: &[&str] = &["label"];
const PAGER_EVENTS: &[EventSpec] = &[EventSpec {
    name: "page-change",
    payload: EventPayload::Unit,
    condition: EventCondition::Always,
}];

const IMG_ATTRIBUTES: &[AttributeSpec] = &[
    AttributeSpec {
        name: "src",
        kind: AttributeKind::Text,
    },
    AttributeSpec {
        name: "alt",
        kind: AttributeKind::Text,
    },
    AttributeSpec {
        name: "decorative",
        kind: AttributeKind::Bool,
    },
];
const IMG_REQUIRED: &[&str] = &["src"];
const IMG_CONSTRAINTS: &[Constraint] = &[Constraint::ExactlyOneAttribute(&["alt", "decorative"])];

const VIDEO_ATTRIBUTES: &[AttributeSpec] = &[
    AttributeSpec {
        name: "src",
        kind: AttributeKind::Text,
    },
    AttributeSpec {
        name: "poster",
        kind: AttributeKind::Text,
    },
    AttributeSpec {
        name: "label",
        kind: AttributeKind::Text,
    },
    AttributeSpec {
        name: "autoplay",
        kind: AttributeKind::Bool,
    },
    AttributeSpec {
        name: "muted",
        kind: AttributeKind::Bool,
    },
    AttributeSpec {
        name: "loop",
        kind: AttributeKind::Bool,
    },
    AttributeSpec {
        name: "controls",
        kind: AttributeKind::Bool,
    },
    AttributeSpec {
        name: "playsinline",
        kind: AttributeKind::Bool,
    },
];
const VIDEO_REQUIRED: &[&str] = &["src", "label"];

const ICON_ATTRIBUTES: &[AttributeSpec] = &[
    AttributeSpec {
        name: "name",
        kind: AttributeKind::Text,
    },
    AttributeSpec {
        name: "family",
        kind: AttributeKind::Text,
    },
];
const ICON_REQUIRED: &[&str] = &["name"];

const TEXTFIELD_ATTRIBUTES: &[AttributeSpec] = &[
    AttributeSpec {
        name: "value",
        kind: AttributeKind::Text,
    },
    AttributeSpec {
        name: "placeholder",
        kind: AttributeKind::Text,
    },
    AttributeSpec {
        name: "label",
        kind: AttributeKind::Text,
    },
    AttributeSpec {
        name: "disabled",
        kind: AttributeKind::Bool,
    },
];
const TEXTFIELD_REQUIRED: &[&str] = &["label"];
const TEXTFIELD_EVENTS: &[EventSpec] = &[
    EventSpec {
        name: "change",
        payload: EventPayload::TextField("text"),
        condition: EventCondition::Always,
    },
    EventSpec {
        name: "submit",
        payload: EventPayload::Unit,
        condition: EventCondition::Always,
    },
];
const TEXTFIELD_CONSTRAINTS: &[Constraint] = &[Constraint::Controlled {
    attribute: "value",
    event: "change",
}];

const REGION_ATTRIBUTES: &[AttributeSpec] = &[
    AttributeSpec {
        name: "label",
        kind: AttributeKind::Text,
    },
    AttributeSpec {
        name: "supplementary",
        kind: AttributeKind::Bool,
    },
];
const REGION_REQUIRED: &[&str] = &["label"];
const REGION_EVENTS: &[EventSpec] = &[
    EventSpec {
        name: "activate",
        payload: EventPayload::Unit,
        condition: EventCondition::Always,
    },
    EventSpec {
        name: "activate-double",
        payload: EventPayload::Unit,
        condition: EventCondition::Always,
    },
];
const REGION_CONSTRAINTS: &[Constraint] = &[
    Constraint::NoInteractiveDescendants,
    Constraint::AtLeastOneEvent(&["activate", "activate-double"]),
];

const LINK_ATTRIBUTES: &[AttributeSpec] = &[
    AttributeSpec {
        name: "routes",
        kind: AttributeKind::CheckedExpression,
    },
    AttributeSpec {
        name: "to",
        kind: AttributeKind::CheckedExpression,
    },
    AttributeSpec {
        name: "disabled",
        kind: AttributeKind::Bool,
    },
];
const LINK_REQUIRED: &[&str] = &["routes", "to"];
const LINK_EVENTS: &[EventSpec] = &[EventSpec {
    name: "follow",
    payload: EventPayload::Unit,
    condition: EventCondition::Always,
}];
const LINK_CONSTRAINTS: &[Constraint] = &[
    Constraint::AccessibleName { attributes: &[] },
    Constraint::NoInteractiveDescendants,
];

const SURFACE_ATTRIBUTES: &[AttributeSpec] = &[AttributeSpec {
    name: "key",
    kind: AttributeKind::Key,
}];
const SURFACE_REQUIRED: &[&str] = &["key"];

macro_rules! container {
    ($name:literal, $attributes:expr) => {
        ElementSpec {
            name: $name,
            availability: Availability::Native,
            content: ContentModel::Children,
            browser_realization: BrowserRealization::NativeHtml,
            interactive: false,
            attributes: $attributes,
            required_attributes: NONE_REQUIRED,
            events: NONE_EVENTS,
            constraints: NONE_CONSTRAINTS,
        }
    };
}

pub(super) const ELEMENTS: &[ElementSpec] = &[
    container!("main", ARIA_LABEL),
    container!("section", ARIA_LABEL),
    container!("header", ARIA_LABEL),
    container!("h1", ARIA_LABEL),
    container!("h2", ARIA_LABEL),
    container!("p", ARIA_LABEL),
    container!("output", ARIA_LABEL),
    ElementSpec {
        name: "progress",
        availability: Availability::Native,
        content: ContentModel::Children,
        browser_realization: BrowserRealization::NativeHtml,
        interactive: false,
        attributes: PROGRESS_ATTRIBUTES,
        required_attributes: NONE_REQUIRED,
        events: NONE_EVENTS,
        constraints: NONE_CONSTRAINTS,
    },
    container!("label", ARIA_LABEL),
    ElementSpec {
        name: "fieldset",
        availability: Availability::Native,
        content: ContentModel::Children,
        browser_realization: BrowserRealization::NativeHtml,
        interactive: false,
        attributes: FIELDSET_ATTRIBUTES,
        required_attributes: NONE_REQUIRED,
        events: NONE_EVENTS,
        constraints: NONE_CONSTRAINTS,
    },
    container!("legend", ARIA_LABEL),
    ElementSpec {
        name: "button",
        availability: Availability::Native,
        content: ContentModel::Children,
        browser_realization: BrowserRealization::PrimitiveAdapter,
        interactive: true,
        attributes: BUTTON_ATTRIBUTES,
        required_attributes: NONE_REQUIRED,
        events: BUTTON_EVENTS,
        constraints: BUTTON_CONSTRAINTS,
    },
    ElementSpec {
        name: "input",
        availability: Availability::Native,
        content: ContentModel::Void,
        browser_realization: BrowserRealization::NativeHtml,
        interactive: true,
        attributes: INPUT_ATTRIBUTES,
        required_attributes: NONE_REQUIRED,
        events: INPUT_EVENTS,
        constraints: NONE_CONSTRAINTS,
    },
    container!("dl", ARIA_LABEL),
    container!("dt", ARIA_LABEL),
    container!("dd", ARIA_LABEL),
    ElementSpec {
        name: "view",
        availability: Availability::Native,
        content: ContentModel::Children,
        browser_realization: BrowserRealization::PrimitiveAdapter,
        interactive: false,
        attributes: VIEW_ATTRIBUTES,
        required_attributes: NONE_REQUIRED,
        events: NONE_EVENTS,
        constraints: VIEW_CONSTRAINTS,
    },
    ElementSpec {
        name: "scroll",
        availability: Availability::Native,
        content: ContentModel::Children,
        browser_realization: BrowserRealization::PrimitiveAdapter,
        interactive: true,
        attributes: SCROLL_ATTRIBUTES,
        required_attributes: NONE_REQUIRED,
        events: SCROLL_EVENTS,
        constraints: NONE_CONSTRAINTS,
    },
    ElementSpec {
        name: "pager",
        availability: Availability::Native,
        content: ContentModel::Children,
        browser_realization: BrowserRealization::PrimitiveAdapter,
        interactive: true,
        attributes: PAGER_ATTRIBUTES,
        required_attributes: PAGER_REQUIRED,
        events: PAGER_EVENTS,
        constraints: NONE_CONSTRAINTS,
    },
    ElementSpec {
        name: "text",
        availability: Availability::Native,
        content: ContentModel::Children,
        browser_realization: BrowserRealization::PrimitiveAdapter,
        interactive: false,
        attributes: NONE,
        required_attributes: NONE_REQUIRED,
        events: NONE_EVENTS,
        constraints: NONE_CONSTRAINTS,
    },
    ElementSpec {
        name: "img",
        availability: Availability::Native,
        content: ContentModel::Void,
        browser_realization: BrowserRealization::PrimitiveAdapter,
        interactive: false,
        attributes: IMG_ATTRIBUTES,
        required_attributes: IMG_REQUIRED,
        events: NONE_EVENTS,
        constraints: IMG_CONSTRAINTS,
    },
    ElementSpec {
        name: "video",
        availability: Availability::Native,
        content: ContentModel::Void,
        browser_realization: BrowserRealization::PrimitiveAdapter,
        interactive: true,
        attributes: VIDEO_ATTRIBUTES,
        required_attributes: VIDEO_REQUIRED,
        events: NONE_EVENTS,
        constraints: NONE_CONSTRAINTS,
    },
    ElementSpec {
        name: "icon",
        availability: Availability::Native,
        content: ContentModel::Void,
        browser_realization: BrowserRealization::PrimitiveAdapter,
        interactive: false,
        attributes: ICON_ATTRIBUTES,
        required_attributes: ICON_REQUIRED,
        events: NONE_EVENTS,
        constraints: NONE_CONSTRAINTS,
    },
    ElementSpec {
        name: "textfield",
        availability: Availability::Native,
        content: ContentModel::Void,
        browser_realization: BrowserRealization::PrimitiveAdapter,
        interactive: true,
        attributes: TEXTFIELD_ATTRIBUTES,
        required_attributes: TEXTFIELD_REQUIRED,
        events: TEXTFIELD_EVENTS,
        constraints: TEXTFIELD_CONSTRAINTS,
    },
    ElementSpec {
        name: "region",
        availability: Availability::Native,
        content: ContentModel::Children,
        browser_realization: BrowserRealization::PrimitiveAdapter,
        interactive: true,
        attributes: REGION_ATTRIBUTES,
        required_attributes: REGION_REQUIRED,
        events: REGION_EVENTS,
        constraints: REGION_CONSTRAINTS,
    },
    ElementSpec {
        name: "Link",
        availability: Availability::StandardImport,
        content: ContentModel::Children,
        browser_realization: BrowserRealization::CoreExtension,
        interactive: true,
        attributes: LINK_ATTRIBUTES,
        required_attributes: LINK_REQUIRED,
        events: LINK_EVENTS,
        constraints: LINK_CONSTRAINTS,
    },
    ElementSpec {
        name: "Surface",
        availability: Availability::StandardImport,
        content: ContentModel::Children,
        browser_realization: BrowserRealization::CoreExtension,
        interactive: false,
        attributes: SURFACE_ATTRIBUTES,
        required_attributes: SURFACE_REQUIRED,
        events: NONE_EVENTS,
        constraints: NONE_CONSTRAINTS,
    },
];

pub(super) fn element(name: &str) -> Option<&'static ElementSpec> {
    ELEMENTS.iter().find(|element| element.name == name)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn catalogue_names_and_members_are_closed_and_unique() {
        let mut element_names = BTreeSet::new();
        for element in ELEMENTS {
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
                            assert!(
                                attributes.contains(name),
                                "constraint attribute `{name}` is not declared on `<{}>`",
                                element.name
                            );
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
                            assert!(
                                events.contains(name),
                                "constraint event `{name}` is not declared on `<{}>`",
                                element.name
                            );
                        }
                    }
                    Constraint::NeutralListItems {
                        element: item_element,
                    } => {
                        assert_eq!(element.name, "view");
                        assert!(attributes.contains("role"));
                        assert!(ELEMENTS.iter().any(|element| {
                            element.name == *item_element && !element.interactive
                        }));
                    }
                }
            }
        }
    }
}
