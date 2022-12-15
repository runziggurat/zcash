use std::{collections::{BTreeMap, HashSet}, hash::Hash};
use spectre::{edge::Edge};

#[derive(Clone, Debug)]
pub struct NGraph<T> {
    pub edges: HashSet<Edge<T>>,
    index: Option<BTreeMap<T, usize>>,
}

impl<T> Default for NGraph<T>
where
    Edge<T>: Eq + Hash,
    T: Copy + Eq + Hash + Ord,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<T> NGraph<T>
where
    Edge<T>: Eq + Hash,
    T: Copy + Eq + Hash + Ord,
{
    pub fn new() -> Self {
        Self {
            edges: Default::default(),
            index: None,
        }
    }

    /// Inserts an edge into the graph.
    pub fn insert(&mut self, edge: Edge<T>) -> bool {
        let is_inserted = self.edges.insert(edge);

        // Delete the cached objects if the edge was successfully inserted because we can't
        // reliably update them from the new connection alone.
        if is_inserted && self.index.is_some() {
            self.clear_cache()
        }

        is_inserted
    }

    pub fn remove(&mut self, edge: &Edge<T>) -> bool {
        let is_removed = self.edges.remove(edge);

        // Delete the cached objects if the edge was successfully removed because we can't reliably
        // update them from the new connection alone.
        if is_removed && self.index.is_some() {
            self.clear_cache()
        }

        is_removed
    }

    fn vertices_from_edges(&self) -> HashSet<T> {
        let mut vertices: HashSet<T> = HashSet::new();
        for edge in self.edges.iter() {
            // Using a hashset guarantees uniqueness.
            vertices.insert(*edge.source());
            vertices.insert(*edge.target());
        }

        vertices
    }

    pub fn vertex_count(&self) -> usize {
        self.vertices_from_edges().len()
    }

    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }
    fn clear_cache(&mut self) {
        self.index = None;
    }

}