use std::{collections::{BTreeMap, HashSet}, hash::Hash};
use spectre::{edge::Edge};


// pub struct NGraphNode<T> {
//     pub node: T,
//     pub nodes: Vec<u32>
// }

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

    pub fn compute_betweenness_and_closeness (&self, addresses: &Vec<T>) ->  (Vec<u32>, Vec<f64>) {
        //nodes = Vec::new(addresses.len());
        let num_nodes = addresses.len();
        println!("asdf: num_nodes {}", num_nodes);
        let mut node_list: Vec<Vec<usize>> = Vec::new();
        for _ in 0..num_nodes {
            node_list.push(Vec::new());
        }
        // for addr in addresses {
        //     nodes.push(new NGraphNode(addr, Vec::new()));
        // }
        let mut betweenness: Vec<u32> = vec!(0; num_nodes);
        let _total_path_length: Vec<u32> = vec!(0; num_nodes);
        let closeness: Vec<f64> = Vec::new();
        println!("asdf: two");

        // For all our edges, check if the nodes are in the good list
        for edge in self.edges.iter() {
            let source = *edge.source();
            let target = *edge.target();

            let src_result = addresses.iter().position(|&r| r == source);
            if src_result == None {
                continue;
            }

            let tgt_result = addresses.iter().position(|&r| r == target);
            if tgt_result == None {
                continue;
            }

            let src_index = src_result.unwrap();
            let tgt_index = tgt_result.unwrap();
            node_list[src_index].push(tgt_index);
            node_list[tgt_index].push(src_index);
        }
        println!("asdf: three");

        for _i in 0..num_nodes-1 {
            // 1.  for node i, find the shortest path to all nodes i+1 to num_nodes - 1

            // 2.  add this number in the to the appropriate pair of indices
            //     in the total_path_length vector

            // 3. for each path found, in length is greater than 1 (i.e., there are
            //    nodes between the two nodes in question), each one gets their
            //    betweenness value incremented


            // for j in i+1..num_nodes {
            //     // 

            // }
  
        }

        println!("asdf: four");
        println!("node_list len: {}", node_list.len());
        println!("bertweenness len: {}", betweenness.len());
        for i in 0..num_nodes {
            println!("asdf: set between for {}, len is {}", i, node_list[i].len());
            betweenness[i] = node_list[i].len() as u32;
        }
        (betweenness, closeness)

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