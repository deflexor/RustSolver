use std::pin::Pin;
#[allow(unused_imports)]
use std::marker::Unpin;

/**
 * A tree structure
 *
 * instead of linking nodes directly,
 * data is contained in a single arena or vector
 * this way there is a single reference
 * nodes instead contain the index of their parent and children in the arena
 */
pub type NodeId = usize;

#[derive(Debug)]
pub struct Tree<T> {
    nodes: Vec<Node<T>>,
}

#[derive(Debug)]
pub struct Node<T> {
    pub children: Vec<NodeId>,
    parent: Option<NodeId>,
    pub data: T,
}

impl<T> Node<T> {
    pub fn new(parent: Option<NodeId>, data: T) -> Self {
        Node {
            data,
            parent,
            children: Vec::new(),
        }
    }
    pub fn set_parent(&mut self, parent: NodeId) {
        self.parent = Some(parent);
    }
    pub fn add_child(&mut self, child: NodeId) {
        self.children.push(child);
    }
}

impl<T> Tree<T> {
    pub fn new() -> Tree<T> {
        Tree { nodes: Vec::new() }
    }
    pub fn create_node(&mut self, parent: Option<NodeId>, data: T) -> NodeId {
        let index: NodeId = self.nodes.len();
        let node = Node::new(parent, data);
        self.nodes.push(node);
        return index;
    }
    pub fn get_node_mut(&mut self, idx: NodeId) -> &mut Node<T> {
        return &mut self.nodes[idx];
    }
    pub fn get_node(&self, idx: NodeId) -> &Node<T> {
        return &self.nodes[idx];
    }
    pub fn iter(&self) -> std::slice::Iter<'_, Node<T>> {
        self.nodes.iter()
    }
    pub fn len(&self) -> usize {
        self.nodes.len()
    }
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
    /// Pre-order traversal of the subtree rooted at `node`.
    pub fn preorder<'a>(&'a self, node: NodeId, out: &mut Vec<&'a T>) {
        let n = self.get_node(node);
        out.push(&n.data);
        for child in &n.children {
            self.preorder(*child, out);
        }
    }
    /// Push a pin-box of this tree to a stable address. Used by callers that
    /// need to keep a `Pin<&mut Tree<T>>` around for self-referential work.
    pub fn into_pin(self) -> Pin<Box<Tree<T>>>
    where
        T: Unpin,
    {
        Pin::new(Box::new(self))
    }
}
