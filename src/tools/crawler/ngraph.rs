use std::{collections::{BTreeMap, HashSet}, hash::Hash};
use spectre::{edge::Edge};

pub type Vertex = Vec<usize>;
pub type AGraph = Vec<Vertex>;

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

    // fn vertices_from_edges(&self) -> HashSet<T> {
    //     let mut vertices: HashSet<T> = HashSet::new();
    //     for edge in self.edges.iter() {
    //         // Using a hashset guarantees uniqueness.
    //         vertices.insert(*edge.source());
    //         vertices.insert(*edge.target());
    //     }

    //     vertices
    // }

    // pub fn create_agraph(&self, addresses: &Vec<T>) -> AGraph {
    //     let num_nodes = addresses.len();
    //     let mut agraph: AGraph = AGraph::new();
    //     for _ in 0..num_nodes {
    //         agraph.push(Vertex::new());
    //     }

    //     // For all our edges, check if the nodes are in the good list
    //     // We use the value of the addresses to find the index
    //     // From then on, it's all integer indices
    //     for edge in self.edges.iter() {
    //         let source = *edge.source();
    //         let target = *edge.target();

    //         let src_result = addresses.iter().position(|&r| r == source);
    //         if src_result == None {
    //             continue;
    //         }

    //         let tgt_result = addresses.iter().position(|&r| r == target);
    //         if tgt_result == None {
    //             continue;
    //         }

    //         let src_index = src_result.unwrap();
    //         let tgt_index = tgt_result.unwrap();
    //         agraph[src_index].push(tgt_index);
    //         agraph[tgt_index].push(src_index);
    //     }
    //     agraph

    // }

    fn clear_cache(&mut self) {
        self.index = None;
    }

}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new() {
        let _: NGraph<()> = NGraph::new();
    }

    #[test]
    fn doit() {
        let (s0, s1, s2, s3, s4, s5, s6) = ("0", "1", "2", "3", "4", "5", "6");
        let addresses = vec!["0", "1", "2", "3", "4", "5", "6"];
        let mut ngraph: NGraph<&str> = NGraph::new();
        // this graph reproduces the image at:
        // https://www.sotr.blog/articles/breadth-first-search
        ngraph.insert(Edge::new(s0, s3));
        ngraph.insert(Edge::new(s0, s5));
        ngraph.insert(Edge::new(s5, s1));
        ngraph.insert(Edge::new(s1, s2));
        ngraph.insert(Edge::new(s2, s4));
        ngraph.insert(Edge::new(s2, s6));
        ngraph.insert(Edge::new(s1, s3));

        let (betweenness, closeness) = ngraph.compute_betweenness_and_closeness(&addresses);
        println!("1: betweenness: {:?}", betweenness);
        println!("1: closeness: {:?}", closeness);
    }

    #[test]
    fn star_graph_a() {
        // center is 0
        let (s0, s1, s2, s3, s4, s5, s6, s7) = ("0", "1", "2", "3", "4", "5", "6", "7");
        let addresses = vec!["0", "1", "2", "3", "4", "5", "6", "7"];
        let mut ngraph: NGraph<&str> = NGraph::new();
        ngraph.insert(Edge::new(s0, s1));
        ngraph.insert(Edge::new(s0, s2));
        ngraph.insert(Edge::new(s0, s3));
        ngraph.insert(Edge::new(s0, s4));
        ngraph.insert(Edge::new(s0, s5));
        ngraph.insert(Edge::new(s0, s6));
        ngraph.insert(Edge::new(s0, s7));

        let (betweenness, closeness) = ngraph.compute_betweenness_and_closeness(&addresses);
        println!("2: betweenness: {:?}", betweenness);
        println!("2: closeness: {:?}", closeness);
    }

    #[test]
    fn star_graph_b() {
        // center is 7
        let (s0, s1, s2, s3, s4, s5, s6, s7) = ("0", "1", "2", "3", "4", "5", "6", "7");
        let addresses = vec!["0", "1", "2", "3", "4", "5", "6", "7"];
        let mut ngraph: NGraph<&str> = NGraph::new();
        ngraph.insert(Edge::new(s0, s7));
        ngraph.insert(Edge::new(s1, s7));
        ngraph.insert(Edge::new(s2, s7));
        ngraph.insert(Edge::new(s3, s7));
        ngraph.insert(Edge::new(s4, s7));
        ngraph.insert(Edge::new(s5, s7));
        ngraph.insert(Edge::new(s6, s7));

        let (betweenness, closeness) = ngraph.compute_betweenness_and_closeness(&addresses);
        println!("3: betweenness: {:?}", betweenness);
        println!("3: closeness: {:?}", closeness);
    }

}