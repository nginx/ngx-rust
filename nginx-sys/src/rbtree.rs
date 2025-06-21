use core::ptr;

use crate::bindings::{ngx_rbtree_insert_pt, ngx_rbtree_node_t, ngx_rbtree_t};

/// Get a reference to the beginning of a tree element data structure,
/// considering the link field offset in it.
///
/// # Safety
///
/// `$node` must be a valid pointer to the field `$link` in the struct `$type`
#[macro_export]
macro_rules! ngx_rbtree_data {
    ($node:expr, $type:path, $link:ident) => {
        $node
            .byte_sub(::core::mem::offset_of!($type, $link))
            .cast::<$type>()
    };
}

/// Initializes the RbTree with specified sentinel and insert function.
///
/// # Safety
///
/// All of the pointers passed must be valid.
/// `sentinel` is expected to be valid for the whole lifetime of the `tree`.
///
pub unsafe fn ngx_rbtree_init(
    tree: *mut ngx_rbtree_t,
    sentinel: *mut ngx_rbtree_node_t,
    insert: ngx_rbtree_insert_pt,
) {
    ngx_rbtree_sentinel_init(sentinel);
    (*tree).root = sentinel;
    (*tree).sentinel = sentinel;
    (*tree).insert = insert;
}

/// Marks the tree node as red.
///
/// # Safety
///
/// `node` must be a valid pointer to a [ngx_rbtree_node_t].
#[inline]
pub unsafe fn ngx_rbt_red(node: *mut ngx_rbtree_node_t) {
    (*node).color = 1
}

/// Marks the tree node as black.
///
/// # Safety
///
/// `node` must be a valid pointer to a [ngx_rbtree_node_t].
#[inline]
pub unsafe fn ngx_rbt_black(node: *mut ngx_rbtree_node_t) {
    (*node).color = 0
}

/// Initializes the sentinel node.
///
/// # Safety
///
/// `node` must be a valid pointer to a [ngx_rbtree_node_t].
#[inline]
pub unsafe fn ngx_rbtree_sentinel_init(node: *mut ngx_rbtree_node_t) {
    ngx_rbt_black(node)
}

/// Returns the least (leftmost) node of the tree.
///
/// # Safety
///
/// `node` must be a valid pointer to a [ngx_rbtree_node_t].
/// `sentinel` must be a valid pointer to the sentinel node in the same Red-Black tree.
#[inline]
pub unsafe fn ngx_rbtree_min(
    mut node: *mut ngx_rbtree_node_t,
    sentinel: *mut ngx_rbtree_node_t,
) -> *mut ngx_rbtree_node_t {
    while !ptr::addr_eq((*node).left, sentinel) {
        node = (*node).left;
    }

    node
}
