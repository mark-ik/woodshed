//! The text seam: which field has the caret.
//!
//! The host owns the caret, the selection overlay, IME, drag selection, and
//! visual (layout-aware) caret movement — but it cannot know where an
//! application keeps its text. This is woodshed's half of that seam: recognize
//! the focused `<input>` by the class of its wrapper, and hand back borrows of
//! the matching [`TextInput`].

use cambium_genet_winit_host::{FocusedTextSlot, Runner};
use layout_dom_api::{LayoutDom as _, LocalName, Namespace};
use woodshed_views::stage::{UiChild, UiState};

use crate::sync::Logic;

/// Woodshed's two editable fields.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    Search,
    CardRename,
}

/// Which field the focused node is, if it is one.
pub fn focused_text(runner: &Runner<UiState, Logic, UiChild>) -> Option<FocusedTextSlot<UiState>> {
    let node = runner.focus()?;
    let field = {
        let dom = runner.dom();
        let dom = dom.borrow();
        if dom.element_name(node)?.local.as_ref() != "input" {
            return None;
        }
        let parent = dom.parent(node)?;
        match dom.attribute(parent, &Namespace::from(""), &LocalName::from("class"))? {
            "search-wrap" => Field::Search,
            "card-rename" => Field::CardRename,
            _ => return None,
        }
    };
    Some(match field {
        Field::Search => FocusedTextSlot {
            node,
            get: Box::new(|ui: &UiState| &ui.search),
            get_mut: Box::new(|ui: &mut UiState| &mut ui.search),
        },
        Field::CardRename => FocusedTextSlot {
            node,
            get: Box::new(|ui: &UiState| &ui.card_rename),
            get_mut: Box::new(|ui: &mut UiState| &mut ui.card_rename),
        },
    })
}
