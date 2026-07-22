use super::{Container, PrincipalBox, impl_principal_layout_element};

/// The principal box of a CSS multi-column formatting context.
///
/// Columns are implementation-positioned children of the composed ordinary
/// box. Keeping the principal box as a distinct layout node lets pagination
/// fragment those column rows without teaching every painter about a second
/// copy of generic container properties.
#[derive(Debug, Clone)]
pub(crate) struct MulticolContainer {
    pub(crate) principal: Container,
}

impl MulticolContainer {
    pub(crate) const fn new(principal: Container) -> Self {
        Self { principal }
    }
}

/// One anonymous column box in a multicol line.
///
/// The document-order index is retained so fragmentation can decide whether a
/// column rule still has content-bearing columns on both sides.
#[derive(Debug, Clone)]
pub(crate) struct MulticolColumn {
    pub(crate) principal: Container,
    pub(crate) index: usize,
}

impl MulticolColumn {
    pub(crate) const fn new(principal: Container, index: usize) -> Self {
        Self { principal, index }
    }
}

impl PrincipalBox for MulticolContainer {
    fn principal(&self) -> &Container {
        &self.principal
    }

    fn principal_mut(&mut self) -> &mut Container {
        &mut self.principal
    }
}

impl PrincipalBox for MulticolColumn {
    fn principal(&self) -> &Container {
        &self.principal
    }

    fn principal_mut(&mut self) -> &mut Container {
        &mut self.principal
    }
}

impl_principal_layout_element!(MulticolContainer, visit_multicol_container);
impl_principal_layout_element!(MulticolColumn, visit_multicol_column);
