use std::{
    collections::HashMap,
    mem,
};

pub use euclid::Rect;
use rustc_hash::FxHashMap;

use crate::{
    custom_measurer::LayoutMeasurer,
    dom_adapter::{
        DOMAdapter,
        LayoutNode,
        NodeKey,
    },
    geometry::Area,
    measure::{
        MeasureContext,
        Phase,
    },
    prelude::{
        AreaModel,
        Gaps,
    },
};

pub struct LayoutMetadata {
    pub root_area: Area,
}

/// Contains the best Root node candidate from where to start measuring
#[derive(PartialEq, Debug, Clone)]
pub enum RootNodeCandidate<Key: NodeKey> {
    /// A valid Node ID
    Valid(Key),

    /// None
    None,
}

impl<Key: NodeKey> RootNodeCandidate<Key> {
    #[must_use]
    pub fn take(&mut self) -> Self {
        mem::replace(self, Self::None)
    }

    /// Propose a new root candidate
    pub fn propose_new_candidate(
        &mut self,
        proposed_candidate: &Key,
        dom_adapter: &mut impl DOMAdapter<Key>,
    ) {
        if let RootNodeCandidate::Valid(current_candidate) = self {
            if current_candidate != proposed_candidate {
                let closest_parent =
                    dom_adapter.closest_common_parent(proposed_candidate, current_candidate);

                if let Some(closest_parent) = closest_parent {
                    *self = RootNodeCandidate::Valid(closest_parent);
                }
            }
        } else {
            *self = RootNodeCandidate::Valid(*proposed_candidate);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DirtyReason {
    None,
    /// Node was moved from one position to another in its parent' children list.
    Reorder,
}

pub struct Torin<Key: NodeKey> {
    /// Layout results of the registered Nodes
    pub results: FxHashMap<Key, LayoutNode>,

    /// Invalid registered nodes since previous layout measurement
    pub dirty: FxHashMap<Key, DirtyReason>,

    /// Best Root node candidate from where to start measuring
    pub root_node_candidate: RootNodeCandidate<Key>,
}

impl<Key: NodeKey> Default for Torin<Key> {
    fn default() -> Self {
        Self::new()
    }
}

impl<Key: NodeKey> Torin<Key> {
    /// Create a new Layout
    pub fn new() -> Self {
        Self {
            results: HashMap::default(),
            dirty: FxHashMap::default(),
            root_node_candidate: RootNodeCandidate::None,
        }
    }

    pub fn size(&self) -> usize {
        self.results.len()
    }

    /// Reset the layout
    pub fn reset(&mut self) {
        self.root_node_candidate = RootNodeCandidate::None;
        self.results.clear();
        self.dirty.clear();
    }

    /// Read the HashSet of dirty nodes
    pub fn get_dirty_nodes(&self) -> &FxHashMap<Key, DirtyReason> {
        &self.dirty
    }

    /// Remove a Node's result and data
    pub fn raw_remove(&mut self, node_id: Key) {
        self.results.remove(&node_id);
        self.dirty.remove(&node_id);
        if let RootNodeCandidate::Valid(id) = self.root_node_candidate {
            if id == node_id {
                self.root_node_candidate = RootNodeCandidate::None;
            }
        }
    }

    /// Remove a Node from the layout
    /// # Panics
    /// Might panic if the parent is not found.
    pub fn remove(
        &mut self,
        node_id: Key,
        dom_adapter: &mut impl DOMAdapter<Key>,
        invalidate_parent: bool,
    ) {
        // Remove itself
        self.raw_remove(node_id);

        // Mark as dirty the Node's parent
        if invalidate_parent {
            self.invalidate(dom_adapter.parent_of(&node_id).unwrap());
        }

        // Remove all it's children
        for child_id in dom_adapter.children_of(&node_id) {
            self.remove(child_id, dom_adapter, false);
        }
    }

    /// Safely mark as dirty a Node, with no reason.
    pub fn safe_invalidate(&mut self, node_id: Key, dom_adapter: &mut impl DOMAdapter<Key>) {
        if dom_adapter.is_node_valid(&node_id) {
            self.dirty.insert(node_id, DirtyReason::None);
        }
    }

    /// Mark as dirty a Node, with no reason.
    pub fn invalidate(&mut self, node_id: Key) {
        self.dirty.insert(node_id, DirtyReason::None);
    }

    /// Mark as dirty a Node, with a reason.
    pub fn invalidate_with_reason(&mut self, node_id: Key, reason: DirtyReason) {
        self.dirty.insert(node_id, reason);
    }

    // Mark as dirty the given Node and all the nodes that depend on it
    pub fn check_dirty_dependants(
        &mut self,
        node_id: Key,
        reason: DirtyReason,
        dom_adapter: &mut impl DOMAdapter<Key>,
        ignore: bool,
    ) {
        if (self.dirty.contains_key(&node_id) && ignore) || !dom_adapter.is_node_valid(&node_id) {
            return;
        }

        // Mark this node as dirty
        self.invalidate(node_id);

        self.root_node_candidate
            .propose_new_candidate(&node_id, dom_adapter);

        // Mark this Node's parent if it is affected
        let parent_id = dom_adapter.parent_of(&node_id);

        if let Some(parent_id) = parent_id {
            let parent = dom_adapter.get_node(&parent_id);

            if let Some(parent) = parent {
                if parent.does_depend_on_inner() {
                    // Mark parent if it depends on it's inner children
                    self.check_dirty_dependants(parent_id, DirtyReason::None, dom_adapter, true);
                } else {
                    let parent_children = dom_adapter.children_of(&parent_id);
                    let multiple_children = parent_children.len() > 1;

                    let mut found_node = match reason {
                        DirtyReason::None => false,
                        // Invalidate all siblings if the node was reordered
                        DirtyReason::Reorder => true,
                    };
                    for child_id in parent_children {
                        if found_node {
                            self.safe_invalidate(child_id, dom_adapter);
                        }
                        if child_id == node_id {
                            found_node = true;
                        }
                    }

                    // Try using the node's parent as root candidate if it has multiple children
                    if multiple_children || parent.do_inner_depend_on_parent() {
                        self.root_node_candidate
                            .propose_new_candidate(&parent_id, dom_adapter);
                    }
                }
            }
        }
    }

    /// Get the Root Node candidate
    pub fn get_root_candidate(&self) -> RootNodeCandidate<Key> {
        self.root_node_candidate.clone()
    }

    /// Find the best root Node from where to start measuring
    pub fn find_best_root(&mut self, dom_adapter: &mut impl DOMAdapter<Key>) {
        if self.results.is_empty() {
            return;
        }
        for (id, reason) in self.dirty.clone() {
            self.check_dirty_dependants(id, reason, dom_adapter, false);
        }
    }

    /// Measure dirty Nodes
    /// # Panics
    /// Might panic if the final root node is not found.
    pub fn measure(
        &mut self,
        suggested_root_id: Key,
        root_area: Area,
        measurer: &mut Option<impl LayoutMeasurer<Key>>,
        dom_adapter: &mut impl DOMAdapter<Key>,
    ) {
        // If there are previosuly cached results
        // But no dirty nodes, we can simply skip the measurement
        // as this means no changes has been made to the layout
        if self.dirty.is_empty() && !self.results.is_empty() {
            return;
        }

        // Try the Root candidate otherwise use the provided Root
        let root_id = if let RootNodeCandidate::Valid(id) = self.root_node_candidate.take() {
            id
        } else {
            suggested_root_id
        };
        let root_parent_id = dom_adapter.parent_of(&root_id);
        let layout_node = root_parent_id
            .and_then(|root_parent_id| self.get(root_parent_id).cloned())
            .unwrap_or(LayoutNode {
                area: root_area,
                inner_area: root_area,
                margin: Gaps::default(),
                data: None,
            });
        let root = dom_adapter.get_node(&root_id).unwrap();

        #[cfg(debug_assertions)]
        {
            let root_height = dom_adapter.height(&root_id).unwrap();
            tracing::info!(
                "Processing {} dirty nodes and {} cached nodes from a height of {}",
                self.dirty.len(),
                self.results.len(),
                root_height
            );
        }

        let layout_metadata = LayoutMetadata { root_area };

        let mut available_area = layout_node.inner_area;
        if let Some(root_parent_id) = root_parent_id {
            let root_parent = dom_adapter.get_node(&root_parent_id).unwrap();
            available_area.move_with_offsets(&root_parent.offset_x, &root_parent.offset_y);
        }

        let mut measure_context = MeasureContext {
            layout: self,
            layout_metadata,
            dom_adapter,
            measurer,
        };

        let (root_revalidated, mut root_layout_node) = measure_context.measure_node(
            root_id,
            &root,
            &layout_node.inner_area,
            &available_area,
            true,
            false,
            Phase::Final,
        );

        // Cache the root Node results if it was modified
        if root_revalidated {
            // Adjust the size of the area if needed
            root_layout_node.area.adjust_size(&root);

            self.cache_node(root_id, root_layout_node);
        }

        self.dirty.clear();
        self.root_node_candidate = RootNodeCandidate::None;
    }

    /// Get the layout_node of a Node
    pub fn get(&self, node_id: Key) -> Option<&LayoutNode> {
        self.results.get(&node_id)
    }

    /// Cache a Node's layout_node
    pub fn cache_node(&mut self, node_id: Key, layout_node: LayoutNode) {
        self.results.insert(node_id, layout_node);
    }
}
