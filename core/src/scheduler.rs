use enum_map::{Enum, EnumMap};
use mheap::{IndexableHeap, MinHeap, indexable_heap::Idx};
use strum::EnumCount;

pub type Cycle = u64;

#[derive(EnumCount, Enum, Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum EventKind {
    HBlankLeave,
    VBlankLeave,

    HBlankEnter,
    VBlankEnter,
}

pub struct Scheduler {
    now: Cycle,
    events: IndexableHeap<ScheduledEvent, MinHeap>,
    indices: EnumMap<EventKind, Idx<ScheduledEvent>>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ScheduledEvent {
    at: Cycle,
    kind: EventKind,
}

impl Default for Scheduler {
    fn default() -> Self {
        let mut events = IndexableHeap::with_capacity(EventKind::COUNT);
        let indices = EnumMap::from_fn(|kind| {
            events.push(ScheduledEvent {
                kind,
                at: Cycle::MAX,
            })
        });

        Self {
            now: 0,
            events,
            indices,
        }
    }
}

impl Scheduler {
    pub fn now(&self) -> Cycle {
        self.now
    }

    pub fn advance(&mut self, cycles: Cycle) {
        self.now = self.now.saturating_add(cycles);
    }

    pub fn schedule(&mut self, kind: EventKind, delay: Cycle) {
        let deadline = self.now.saturating_add(delay);

        let idx = self.indices[kind];
        self.events.by_index_mut(idx).at = deadline;
    }

    pub fn cancel(&mut self, kind: EventKind) {
        let idx = self.indices[kind];
        self.events.by_index_mut(idx).at = Cycle::MAX;
    }

    pub fn cycles_until_next(&self) -> Cycle {
        match self.events.peek() {
            Some(event) if event.at <= self.now => 0,
            Some(event) => event.at - self.now,
            None => {
                unreachable!("broken contract: empty queue")
            }
        }
    }

    pub fn pop_due(&mut self) -> Option<EventKind> {
        let Some(mut event) = self.events.peek_mut() else {
            unreachable!("broken contract: empty queue")
        };

        if event.at > self.now {
            return None;
        }

        event.at = Cycle::MAX;

        Some(event.kind)
    }
}
