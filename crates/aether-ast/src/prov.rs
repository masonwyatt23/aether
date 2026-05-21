//! Provenance chain types.
//!
//! Every runtime value carries a `ProvChain` head pointing into a shared arena
//! of `ProvNode`s. Each node names the operation that produced a value plus
//! its source `Span` and the parent provenance ids of its inputs.
//!
//! The chain is a DAG of operations rooted at primitive constructors
//! (literals, inputs, axioms). `provenance(v)` returns the chain reachable
//! from `v.prov_head`.

use crate::span::Span;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq)]
pub enum ProvOp {
    /// Literal value in source code.
    Lit,
    /// Identifier read from an environment.
    Var(String),
    /// Built-in or user-defined function call (callee name for readability).
    Call(String),
    /// Binary operator.
    BinOp(String),
    /// Unary operator.
    UnOp(String),
    /// Result of `confident(value, p)`.
    Confident(f64),
    /// Result of `assume(predicate)`.
    Assume,
    /// External tool invocation.
    Tool(String),
    /// Synthetic node (e.g. interpreter-internal).
    Synthetic(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProvNode {
    pub op: ProvOp,
    pub span: Span,
    pub parents: Vec<usize>,
}

/// A handle into a chain. Cheaply cloneable.
#[derive(Debug, Clone)]
pub struct ProvChain {
    arena: Arc<ProvArena>,
    pub head: usize,
}

#[derive(Debug)]
pub struct ProvArena {
    nodes: parking_lot_lite::Mutex<Vec<ProvNode>>,
}

mod parking_lot_lite {
    //! Tiny std-only Mutex shim so we avoid an external dep for the AST crate.
    //! For the interpreter's hot loop we'll keep allocations bounded; provenance
    //! is opt-out via `@no_prov` and tests for performance characteristics live
    //! in `aether-eval`.
    use std::cell::UnsafeCell;
    use std::ops::{Deref, DerefMut};
    use std::sync::atomic::{AtomicBool, Ordering};

    pub struct Mutex<T> {
        locked: AtomicBool,
        data: UnsafeCell<T>,
    }

    unsafe impl<T: Send> Send for Mutex<T> {}
    unsafe impl<T: Send> Sync for Mutex<T> {}

    impl<T> Mutex<T> {
        pub const fn new(t: T) -> Self {
            Self {
                locked: AtomicBool::new(false),
                data: UnsafeCell::new(t),
            }
        }
        pub fn lock(&self) -> Guard<'_, T> {
            while self
                .locked
                .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_err()
            {
                std::hint::spin_loop();
            }
            Guard { m: self }
        }
    }

    impl<T: std::fmt::Debug> std::fmt::Debug for Mutex<T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("Mutex").finish_non_exhaustive()
        }
    }

    pub struct Guard<'a, T> {
        m: &'a Mutex<T>,
    }
    impl<T> Deref for Guard<'_, T> {
        type Target = T;
        fn deref(&self) -> &T {
            unsafe { &*self.m.data.get() }
        }
    }
    impl<T> DerefMut for Guard<'_, T> {
        fn deref_mut(&mut self) -> &mut T {
            unsafe { &mut *self.m.data.get() }
        }
    }
    impl<T> Drop for Guard<'_, T> {
        fn drop(&mut self) {
            self.m.locked.store(false, Ordering::Release);
        }
    }
}

impl ProvArena {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            nodes: parking_lot_lite::Mutex::new(Vec::new()),
        })
    }

    pub fn alloc(&self, node: ProvNode) -> usize {
        let mut g = self.nodes.lock();
        let id = g.len();
        g.push(node);
        id
    }

    pub fn get(&self, id: usize) -> Option<ProvNode> {
        let g = self.nodes.lock();
        g.get(id).cloned()
    }

    pub fn len(&self) -> usize {
        self.nodes.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for ProvArena {
    fn default() -> Self {
        Self {
            nodes: parking_lot_lite::Mutex::new(Vec::new()),
        }
    }
}

impl ProvChain {
    pub fn singleton(arena: Arc<ProvArena>, op: ProvOp, span: Span) -> Self {
        let head = arena.alloc(ProvNode {
            op,
            span,
            parents: vec![],
        });
        Self { arena, head }
    }

    pub fn extend(arena: Arc<ProvArena>, op: ProvOp, span: Span, parents: Vec<usize>) -> Self {
        let head = arena.alloc(ProvNode { op, span, parents });
        Self { arena, head }
    }

    pub fn arena(&self) -> &Arc<ProvArena> {
        &self.arena
    }

    /// Walk the DAG from `head` in topological-order (parents before children
    /// of the head as seen by reverse-postorder DFS). Returns owned nodes.
    pub fn nodes_topo(&self) -> Vec<(usize, ProvNode)> {
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut stack = vec![self.head];
        let mut order = Vec::new();
        while let Some(id) = stack.pop() {
            if !seen.insert(id) {
                continue;
            }
            order.push(id);
            if let Some(n) = self.arena.get(id) {
                for &p in &n.parents {
                    stack.push(p);
                }
            }
        }
        order.reverse();
        for id in order {
            if let Some(n) = self.arena.get(id) {
                out.push((id, n));
            }
        }
        out
    }
}

impl PartialEq for ProvChain {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.arena, &other.arena) && self.head == other.head
    }
}
