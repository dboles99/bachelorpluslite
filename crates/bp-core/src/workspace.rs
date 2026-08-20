//! The set of open documents and which one is in front.

use std::path::PathBuf;

use time::OffsetDateTime;

use crate::{Document, DocumentId};

/// Every document currently open, in tab order.
///
/// A `Vec` rather than a map: tab counts are small, order is user-visible and
/// must be preserved, and iteration in tab order is the common operation.
#[derive(Debug, Clone, Default)]
pub struct Workspace {
    docs: Vec<Document>,
    active: Option<DocumentId>,
    next_id: u64,
}

impl Workspace {
    pub fn new() -> Self {
        Self::default()
    }

    fn alloc_id(&mut self) -> DocumentId {
        // Ids are never reused, so a stale id from a closed tab resolves to
        // `None` instead of silently addressing whatever took its place.
        self.next_id += 1;
        DocumentId::new(self.next_id)
    }

    /// Open a new empty document and make it active.
    pub fn open_new(&mut self, created_at: OffsetDateTime) -> DocumentId {
        let id = self.alloc_id();
        self.docs.push(Document::new(id, created_at));
        self.active = Some(id);
        id
    }

    /// Open a document from a path and make it active.
    pub fn open_path(&mut self, path: PathBuf, created_at: OffsetDateTime) -> DocumentId {
        let id = self.alloc_id();
        self.docs.push(Document::opened(id, path, created_at));
        self.active = Some(id);
        id
    }

    /// Close a document.
    ///
    /// Returns the closed document so the caller can decide what to do about
    /// unsaved changes; this method does not itself prompt or discard.
    /// Activation moves to the neighbour on the right, else the left, which
    /// is what tabbed editors do.
    pub fn close(&mut self, id: DocumentId) -> Option<Document> {
        let index = self.docs.iter().position(|d| d.id() == id)?;
        let closed = self.docs.remove(index);

        if self.active == Some(id) {
            self.active = self
                .docs
                .get(index)
                .or_else(|| index.checked_sub(1).and_then(|i| self.docs.get(i)))
                .map(Document::id);
        }
        Some(closed)
    }

    pub fn get(&self, id: DocumentId) -> Option<&Document> {
        self.docs.iter().find(|d| d.id() == id)
    }

    pub fn get_mut(&mut self, id: DocumentId) -> Option<&mut Document> {
        self.docs.iter_mut().find(|d| d.id() == id)
    }

    pub const fn active_id(&self) -> Option<DocumentId> {
        self.active
    }

    pub fn active(&self) -> Option<&Document> {
        self.active.and_then(|id| self.get(id))
    }

    pub fn active_mut(&mut self) -> Option<&mut Document> {
        self.active.and_then(move |id| self.get_mut(id))
    }

    /// Focus a document. Ignores ids that are not open.
    pub fn set_active(&mut self, id: DocumentId) -> bool {
        let exists = self.docs.iter().any(|d| d.id() == id);
        if exists {
            self.active = Some(id);
        }
        exists
    }

    /// Documents in tab order.
    pub fn iter(&self) -> impl Iterator<Item = &Document> {
        self.docs.iter()
    }

    pub fn len(&self) -> usize {
        self.docs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.docs.is_empty()
    }

    /// Documents with unsaved changes -- what a close-all prompt needs.
    pub fn dirty(&self) -> impl Iterator<Item = &Document> {
        self.docs.iter().filter(|d| d.is_dirty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    const T: OffsetDateTime = datetime!(2026-08-16 09:00 UTC);

    #[test]
    fn opening_documents_makes_them_active_in_order() {
        let mut ws = Workspace::new();
        let a = ws.open_new(T);
        let b = ws.open_new(T);

        assert_eq!(ws.len(), 2);
        assert_eq!(ws.active_id(), Some(b));
        assert_eq!(ws.iter().map(Document::id).collect::<Vec<_>>(), vec![a, b]);
    }

    #[test]
    fn ids_are_not_reused_after_close() {
        let mut ws = Workspace::new();
        let a = ws.open_new(T);
        ws.close(a);
        let b = ws.open_new(T);

        assert_ne!(a, b);
        assert!(ws.get(a).is_none(), "a stale id must not resolve");
    }

    #[test]
    fn closing_the_active_tab_activates_the_right_neighbour() {
        let mut ws = Workspace::new();
        let a = ws.open_new(T);
        let b = ws.open_new(T);
        let c = ws.open_new(T);

        ws.set_active(b);
        ws.close(b);
        assert_eq!(ws.active_id(), Some(c));
        assert_eq!(ws.iter().map(Document::id).collect::<Vec<_>>(), vec![a, c]);
    }

    #[test]
    fn closing_the_last_tab_falls_back_leftwards() {
        let mut ws = Workspace::new();
        let a = ws.open_new(T);
        let b = ws.open_new(T);

        ws.set_active(b);
        ws.close(b);
        assert_eq!(ws.active_id(), Some(a));
    }

    #[test]
    fn closing_the_only_tab_leaves_nothing_active() {
        let mut ws = Workspace::new();
        let a = ws.open_new(T);
        ws.close(a);

        assert!(ws.is_empty());
        assert_eq!(ws.active_id(), None);
        assert!(ws.active().is_none());
    }

    #[test]
    fn closing_an_inactive_tab_does_not_move_focus() {
        let mut ws = Workspace::new();
        let a = ws.open_new(T);
        let b = ws.open_new(T);

        ws.close(a);
        assert_eq!(ws.active_id(), Some(b));
    }

    #[test]
    fn set_active_rejects_unknown_ids() {
        let mut ws = Workspace::new();
        let a = ws.open_new(T);
        let ghost = DocumentId::new(999);

        assert!(!ws.set_active(ghost));
        assert_eq!(ws.active_id(), Some(a));
    }

    #[test]
    fn dirty_lists_only_unsaved_documents() {
        let mut ws = Workspace::new();
        let a = ws.open_new(T);
        let _b = ws.open_new(T);
        ws.get_mut(a).unwrap().mark_modified();

        assert_eq!(ws.dirty().map(Document::id).collect::<Vec<_>>(), vec![a]);
    }
}
