//! Page-level paint scheduling for fragmented content.
//!
//! A fixed-size box can contribute overflow on a later fragmentainer without
//! occupying normal flow there. Ordinary content on that page supplies the
//! backdrop, so those continuations paint after main-flow boxes. Flex boxes
//! additionally expose separate decoration/content phases: their decoration
//! belongs to the backdrop while their item content stays above the overflow.

use crate::layout::elements::{LayoutElement, LayoutNode, LayoutVisitor, PageContentRole};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ElementPaintPhase {
    All,
    Decoration,
    Contents,
}

impl ElementPaintPhase {
    pub(super) const fn paints_decoration(self) -> bool {
        matches!(self, Self::All | Self::Decoration)
    }

    pub(super) const fn paints_contents(self) -> bool {
        matches!(self, Self::All | Self::Contents)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PlannedPageElement {
    pub(super) index: usize,
    pub(super) phase: ElementPaintPhase,
}

impl PlannedPageElement {
    const fn new(index: usize, phase: ElementPaintPhase) -> Self {
        Self { index, phase }
    }
}

#[derive(Clone, Copy)]
struct PaintParticipant {
    role: PageContentRole,
    supports_phases: bool,
}

pub(super) fn plan_page_elements(elements: &[(f32, LayoutNode)]) -> Vec<PlannedPageElement> {
    let participants = elements
        .iter()
        .map(|(_, element)| PaintParticipant {
            role: element.page_content_role(),
            supports_phases: supports_phased_paint(element.as_ref()),
        })
        .collect::<Vec<_>>();
    plan_participants(&participants)
}

fn plan_participants(participants: &[PaintParticipant]) -> Vec<PlannedPageElement> {
    let has_overflow = participants
        .iter()
        .any(|participant| participant.role == PageContentRole::OverflowContinuation);
    if !has_overflow {
        return (0..participants.len())
            .map(|index| PlannedPageElement::new(index, ElementPaintPhase::All))
            .collect();
    }

    let mut plan = Vec::with_capacity(participants.len() * 2);
    for (index, participant) in participants.iter().enumerate() {
        if participant.supports_phases {
            plan.push(PlannedPageElement::new(
                index,
                ElementPaintPhase::Decoration,
            ));
        } else if participant.role != PageContentRole::OverflowContinuation {
            plan.push(PlannedPageElement::new(index, ElementPaintPhase::All));
        }
    }
    for (index, participant) in participants.iter().enumerate() {
        if participant.supports_phases {
            plan.push(PlannedPageElement::new(index, ElementPaintPhase::Contents));
        } else if participant.role == PageContentRole::OverflowContinuation {
            plan.push(PlannedPageElement::new(index, ElementPaintPhase::All));
        }
    }
    plan
}

fn supports_phased_paint(element: &dyn LayoutElement) -> bool {
    #[derive(Default)]
    struct PhaseSupport(Option<bool>);

    impl LayoutVisitor for PhaseSupport {
        fn visit_container(&mut self, element: &crate::layout::elements::Container) {
            self.0 = Some(
                element.paint.group.is_identity()
                    && element.paint.background.layers.blur_radius == 0.0
                    && element.paint.filter.is_none(),
            );
        }

        fn visit_flex_row(&mut self, element: &crate::layout::elements::FlexRow) {
            self.0 = Some(
                element.paint.group.is_identity()
                    && element.paint.background.layers.blur_radius == 0.0
                    && element.paint.filter.is_none(),
            );
        }
    }

    let mut visitor = PhaseSupport::default();
    element.accept(&mut visitor);
    visitor.0.unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overflow_continuations_paint_after_the_main_flow_backdrop() {
        let participants = [
            PaintParticipant {
                role: PageContentRole::OverflowContinuation,
                supports_phases: false,
            },
            PaintParticipant {
                role: PageContentRole::MainFlow,
                supports_phases: false,
            },
        ];

        assert_eq!(
            plan_participants(&participants),
            vec![
                PlannedPageElement::new(1, ElementPaintPhase::All),
                PlannedPageElement::new(0, ElementPaintPhase::All),
            ]
        );
    }

    #[test]
    fn phase_capable_main_flow_keeps_contents_above_overflow() {
        let participants = [
            PaintParticipant {
                role: PageContentRole::OverflowContinuation,
                supports_phases: false,
            },
            PaintParticipant {
                role: PageContentRole::MainFlow,
                supports_phases: true,
            },
        ];

        assert_eq!(
            plan_participants(&participants),
            vec![
                PlannedPageElement::new(1, ElementPaintPhase::Decoration),
                PlannedPageElement::new(0, ElementPaintPhase::All),
                PlannedPageElement::new(1, ElementPaintPhase::Contents),
            ]
        );
    }

    #[test]
    fn phase_capable_boxes_keep_css_decoration_and_content_order() {
        let participants = [
            PaintParticipant {
                role: PageContentRole::OverflowContinuation,
                supports_phases: true,
            },
            PaintParticipant {
                role: PageContentRole::MainFlow,
                supports_phases: true,
            },
        ];

        assert_eq!(
            plan_participants(&participants),
            vec![
                PlannedPageElement::new(0, ElementPaintPhase::Decoration),
                PlannedPageElement::new(1, ElementPaintPhase::Decoration),
                PlannedPageElement::new(0, ElementPaintPhase::Contents),
                PlannedPageElement::new(1, ElementPaintPhase::Contents),
            ]
        );
    }
}
