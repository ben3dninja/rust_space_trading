use bevy::{prelude::*, utils::HashMap};
use ratatui::{
    layout::Alignment,
    style::{Style, Stylize},
    text::{Line, Span},
    widgets::{block::Title, Block, List, ListState, StatefulWidget, StatefulWidgetRef},
};

use crate::utils::Direction2;
use crate::{objects::prelude::*, utils::list::ClampedList};

#[derive(Debug, Event)]
pub enum TreeEvent {
    Select(Direction2),
    ToggleTreeExpansion,
}

#[derive(Debug, Clone)]
struct TreeEntry {
    id: BodyID,
    name: String,
    is_last_child: bool,
    index_of_parent: Option<usize>,
    is_expanded: bool,
}

#[derive(Debug)]
pub struct TreeState {
    /// Indices of the entries in the system tree, and whether they are expanded or not
    visible_tree_entries: Vec<usize>,
    system_tree: Vec<TreeEntry>,
    focus_body: Option<BodyID>,
    list_state: ListState,
}

impl ClampedList for TreeState {
    fn list_state(&mut self) -> &mut ListState {
        &mut self.list_state
    }

    fn len(&self) -> usize {
        self.visible_tree_entries.len()
    }
}

pub struct TreeWidget;

impl StatefulWidget for TreeWidget {
    type State = TreeState;
    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
        state: &mut Self::State,
    ) where
        Self: Sized,
    {
        let texts: Vec<Line<'_>> = state
            .visible_tree_entries
            .iter()
            .map(|&index_in_tree| {
                let entry = &state.system_tree[index_in_tree];
                let style = if Some(entry.id) == state.focus_body {
                    Style::default().bold()
                } else {
                    Style::default()
                };
                vec![
                    Span::from(state.build_deepness_prefix(index_in_tree)),
                    Span::styled(entry.name.clone(), style),
                ]
                .into()
            })
            .collect();
        let list = List::new(texts)
            .block(
                Block::bordered()
                    .title(Title::from("Tree view".bold()).alignment(Alignment::Center)),
            )
            .highlight_symbol("> ");
        <List as StatefulWidgetRef>::render_ref(&list, area, buf, &mut state.list_state)
    }
}

impl TreeState {
    pub fn new<'a>(
        primary: &'a BodyData,
        focus_body: Option<&'a BodyData>,
        bodies: impl Iterator<Item = &'a BodyData>,
    ) -> TreeState {
        #[derive(Clone)]
        struct Temp {
            children: Vec<BodyID>,
            semimajor_axis: f64,
            name: String,
        }
        let mut info: HashMap<BodyID, Temp> = bodies
            .map(|data| {
                (
                    data.id,
                    Temp {
                        children: data.orbiting_bodies.clone(),
                        semimajor_axis: data.semimajor_axis,
                        name: data.name.clone(),
                    },
                )
            })
            .collect();
        let info_bis = info.clone();
        for entry in info.values_mut() {
            entry.children.retain(|b| info_bis.get(b).is_some());
            entry.children.sort_by(|a, b| {
                info_bis[a]
                    .semimajor_axis
                    .total_cmp(&info_bis[b].semimajor_axis)
            });
        }
        fn fill_tree_rec(
            tree: &mut Vec<TreeEntry>,
            info: &HashMap<BodyID, Temp>,
            id: BodyID,
            index_of_parent: Option<usize>,
            is_last_child: bool,
        ) {
            let Temp { children, name, .. } = &info[&id];
            tree.push(TreeEntry {
                id,
                name: name.clone(),
                is_last_child,
                index_of_parent,
                is_expanded: false,
            });
            let index_of_parent = tree.len() - 1;
            let size = children.len();
            for (i, &child) in children.iter().enumerate() {
                fill_tree_rec(tree, info, child, Some(index_of_parent), i == size - 1);
            }
        }
        let mut system_tree = Vec::new();
        fill_tree_rec(&mut system_tree, &info, primary.id, None, true);

        TreeState {
            system_tree,
            visible_tree_entries: vec![0],
            focus_body: focus_body.map(|r| r.id),
            list_state: ListState::default().with_selected(Some(0)),
        }
    }
}

impl TreeState {
    /// Each entry of this vector corresponds to an ancestor of the body.
    /// The body itself is the last entry, and the primary body is the first.
    /// Each boolean corresponds to whether or not the ancestor is a last child
    /// so that we can decide which symbol we display
    pub fn compute_deepness_map(&self, index_in_tree: usize) -> Vec<bool> {
        let tree = &self.system_tree;
        let mut current = &tree[index_in_tree];
        let mut map = vec![current.is_last_child];
        while let Some(i) = current.index_of_parent {
            current = &tree[i];
            map.push(current.is_last_child);
        }
        map.reverse();
        map
    }

    fn build_deepness_prefix(&self, index_in_tree: usize) -> String {
        let mut prefix = String::new();
        let deepness_map = self.compute_deepness_map(index_in_tree);
        let last = deepness_map.len().saturating_sub(2);
        for (i, &is_last) in deepness_map.iter().skip(1).enumerate() {
            prefix.push_str(if is_last {
                if i == last {
                    "└─"
                } else {
                    "  "
                }
            } else if i == last {
                "├─"
            } else {
                "│ "
            });
        }
        prefix
    }

    /// Returns the index of the specified body in the system tree,
    /// or None if the body is not present
    pub fn index_of(&self, id: BodyID) -> Option<usize> {
        self.system_tree.iter().position(|entry| entry.id == id)
    }

    fn nth_visible_entry(&self, n: usize) -> Option<&TreeEntry> {
        self.visible_tree_entries
            .get(n)
            .map(|&i| &self.system_tree[i])
    }

    pub fn index_of_nth_visible_entry(&self, n: usize) -> Option<usize> {
        self.nth_visible_entry(n)
            .and_then(|entry| self.index_of(entry.id))
    }

    pub fn toggle_selection_expansion(&mut self) {
        if let Some(index) = self.list_state.selected() {
            self.toggle_visible_entry_expansion(index);
        }
    }

    pub fn expand_visible_entry_by_id(&mut self, id: BodyID) {
        if let Some(i) = self.index_of(id) {
            self.try_expand_visible_entry(i);
        }
    }

    fn toggle_visible_entry_expansion(&mut self, index: usize) {
        if index < self.visible_tree_entries.len() && !self.try_collapse_visible_entry(index) {
            self.try_expand_visible_entry(index);
        }
    }

    pub fn try_expand_visible_entry(&mut self, index: usize) -> bool {
        let index_in_tree = self.visible_tree_entries[index];
        if self.system_tree[index_in_tree].is_expanded {
            return false;
        }
        self.system_tree[index_in_tree].is_expanded = true;
        let mut i = index_in_tree;
        let mut to_add = Vec::new();
        while let Some(parent) = self
            .system_tree
            .get_mut(i + 1)
            .and_then(|next| next.index_of_parent)
        {
            if parent < index_in_tree {
                break;
            }
            i += 1;
            if self.system_tree[parent].is_expanded {
                to_add.push(i);
            }
        }
        let end = self.visible_tree_entries.split_off(index + 1);
        self.visible_tree_entries.extend(to_add);
        self.visible_tree_entries.extend(end);
        true
    }

    pub fn try_collapse_visible_entry(&mut self, index: usize) -> bool {
        let index_in_tree = self.visible_tree_entries[index];
        if !self.system_tree[index_in_tree].is_expanded {
            return false;
        }
        let mut i = 0;
        for &next_index in self.visible_tree_entries[(index + 1)..].iter() {
            let next = &self.system_tree[next_index];
            if let Some(parent) = next.index_of_parent {
                if parent >= index_in_tree {
                    i += 1;
                    continue;
                }
            }
            break;
        }
        self.visible_tree_entries
            .drain((index + 1)..(index + i + 1));
        self.system_tree[index_in_tree].is_expanded = false;
        true
    }

    pub fn selected_body_id(&self) -> BodyID {
        self.nth_visible_entry(self.list_state.selected().unwrap())
            .unwrap()
            .id
    }

    pub fn try_expand_entry(&mut self, index: usize) {
        let mut ancestors = Vec::new();
        let mut current = &self.system_tree[index];
        while let Some(parent_index_in_tree) = current.index_of_parent {
            ancestors.push(parent_index_in_tree);
            current = &self.system_tree[parent_index_in_tree];
        }
        ancestors.reverse();
        for index_in_tree in ancestors {
            if let Some(index) = self
                .visible_tree_entries
                .iter()
                .position(|&i| i == index_in_tree)
            {
                self.try_expand_visible_entry(index);
            }
        }
    }

    pub fn select_body(&mut self, id: BodyID) -> bool {
        if let Some(index) = self.index_of(id) {
            self.try_expand_entry(index);
            if let Some(index) = self
                .visible_tree_entries
                .iter()
                .position(|&index_in_tree| self.system_tree[index_in_tree].id == id)
            {
                self.list_state.select(Some(index));
                return true;
            }
        }
        false
    }

    pub fn focus_body(&mut self, id: BodyID) {
        if let Some(index) = self.index_of(id) {
            self.try_expand_entry(index);
            self.focus_body = Some(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{prelude::*, ui::screen::explorer::ExplorerContext};
    use bevy::app::App;

    fn new_app(moons: bool) -> App {
        let mut app = App::new();
        let mut client = ClientPlugin::testing().in_mode(ClientMode::Explorer);
        if moons {
            client = client.with_bodies(BodiesConfig::SmallestBodyType(BodyType::Moon));
        }
        app.add_plugins((client, TuiPlugin::testing()));
        // One update to set client mode
        app.update();
        // One update to build system
        app.update();
        // One update to build context
        app.update();
        app
    }

    #[test]
    fn test_initialize_tree() {
        let app = new_app(false);
        let world = app.world();
        let ctx = world.resource::<ExplorerContext>();
        let tree = &ctx.tree_state;
        assert_eq!(tree.system_tree.len(), 9);
        assert!(tree.system_tree[0].is_last_child);
        assert!(tree.system_tree[8].is_last_child);
        for i in 1..8 {
            assert!(!tree.system_tree[i].is_last_child);
        }
    }
    #[test]
    fn test_select_body() {
        let mut app = new_app(false);
        let world = app.world_mut();
        let mut ctx = world.resource_mut::<ExplorerContext>();
        let tree = &mut ctx.tree_state;
        let earth = id_from("terre");
        tree.select_body(earth);
        assert_eq!(tree.selected_body_id(), earth)
    }

    #[test]
    fn test_toggle_entry_expansion() {
        let mut app = new_app(false);
        let world = app.world_mut();
        let mut ctx = world.resource_mut::<ExplorerContext>();
        let tree = &mut ctx.tree_state;
        tree.toggle_selection_expansion();
        assert_eq!(tree.visible_tree_entries.len(), 9);
        assert!(tree.nth_visible_entry(0).unwrap().is_expanded);
        for i in 1..9 {
            tree.toggle_visible_entry_expansion(i);
        }
        assert_eq!(tree.visible_tree_entries.len(), 9);
        tree.toggle_selection_expansion();
        assert_eq!(tree.visible_tree_entries.len(), 1);
        assert!(!tree.nth_visible_entry(0).unwrap().is_expanded);
    }

    #[test]
    fn test_deepness_map() {
        let mut app = new_app(true);
        app.update();
        app.update();
        let world = app.world();
        let ctx = world.resource::<ExplorerContext>();
        let tree = &ctx.tree_state;

        assert_eq!(
            tree.compute_deepness_map(tree.index_of(id_from("lune")).unwrap()),
            vec![true, false, true]
        );
    }

    #[test]
    fn test_build_deepness_prefix() {
        let mut app = new_app(true);
        let world = app.world_mut();
        let mut ctx = world.resource_mut::<ExplorerContext>();
        let tree_state = &mut ctx.tree_state;
        assert_eq!(tree_state.build_deepness_prefix(0), "");
        tree_state.toggle_selection_expansion();
        assert_eq!(
            tree_state.build_deepness_prefix(tree_state.index_of_nth_visible_entry(8).unwrap()),
            "└─"
        );
        tree_state.toggle_visible_entry_expansion(8);
        assert_eq!(
            tree_state.build_deepness_prefix(tree_state.index_of_nth_visible_entry(9).unwrap()),
            "  ├─"
        );
    }
}
