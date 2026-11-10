//! The VM's model: its value objects, threads and every function's
//! activation, which persist between runs.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// A GPL value. A function is its handle in the program's map, valid
/// while the program is the one saved with ([`Vm::program`]).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Value {
    Null,
    Integer(i32),
    /// Fixed point, as the VM holds it.
    Float(i32),
    String(String),
    List(List),
    Agent(u32),
    Boolean(bool),
    Coord(i32, i32),
    Function(u32),
}

/// A list: its elements, each node's number (`gpl::ops::node`), and the
/// freed nodes a cursor may still read.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct List {
    pub items: Vec<Value>,
    pub nodes: Vec<u32>,
    pub freed: Vec<Freed>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Freed {
    pub node: u32,
    pub value: Value,
    pub next: Option<u32>,
}

/// A thread's mode: the VM's thread modes, by name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    Off,
    Once,
    Repeat,
}

/// A function object's thread fields.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Thread {
    pub frame: String,
    pub interval: i32,
    pub mode: Mode,
    pub suspended: bool,
    pub left: i32,
    pub running: bool,
    pub pending: Option<u32>,
    pub last: u32,
    pub args: Vec<Value>,
}

/// A `foreach` cursor: free, on a node, or at the end.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cursor {
    Free,
    At { list: u32, node: u32 },
    End,
}

/// One function's values: three object handles a record, and its
/// cursors. Its links are the program's, rebuilt.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Activation {
    pub handles: Vec<Option<u32>>,
    pub cursors: Vec<Cursor>,
}

/// The VM between runs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vm {
    /// The program's fingerprint: its function handles and activations
    /// hold only under it.
    pub program: u64,
    /// Every object by id; a freed one's slot stays.
    pub objects: Vec<Value>,
    pub threads: BTreeMap<u32, Thread>,
    pub free: Vec<u32>,
    /// By definition index, those made.
    pub activations: BTreeMap<u32, Activation>,
    pub top: Vec<Option<u32>>,
    pub error_line: i32,
    pub count: u64,
    /// The GPL RNG's state.
    pub rng: u32,
    pub steps: u64,
}
