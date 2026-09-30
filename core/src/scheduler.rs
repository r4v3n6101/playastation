use alloc::collections::BinaryHeap;
use core::cmp::Reverse;

use strum::{EnumCount, EnumIter, IntoEnumIterator};

/// This is CPU time in cycles.
pub type SystemCycle = u64;

#[derive(Debug, Clone)]
pub struct Scheduler {
    now: SystemCycle,
    generations: [u64; Event::COUNT],
    events: BinaryHeap<Reverse<QueuedEvent>>,
}

#[derive(EnumCount, EnumIter, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Event {
    /// GPU and timers together.
    Gpu,
    CdRom,
    Joy,
    Dma,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct QueuedEvent {
    scheduled_at: SystemCycle,
    deadline: SystemCycle,
    event: Event,
    generation: u64,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self {
            now: 0,
            generations: [0; _],
            events: Event::iter()
                .map(|event| QueuedEvent {
                    scheduled_at: 0,
                    deadline: 0,
                    generation: 0,
                    event,
                })
                .map(Reverse)
                .collect(),
        }
    }
}

impl Scheduler {
    pub fn advance(&mut self, cycles: SystemCycle) {
        self.now = self
            .now
            .checked_add(cycles)
            .expect("scheduler time overflow");
    }

    pub fn cycles_till_next_event(&mut self) -> Option<SystemCycle> {
        self.discard_stale();
        self.events
            .peek()
            .map(|entry| entry.0.deadline.saturating_sub(self.now))
    }

    pub fn schedule(&mut self, event: Event, after: SystemCycle) {
        let deadline = self
            .now
            .checked_add(after)
            .expect("event deadline overflow");

        self.cancel(event);
        self.events.push(Reverse(QueuedEvent {
            deadline,
            event,
            generation: self.generations[event as usize],
            scheduled_at: self.now,
        }));
    }

    pub fn cancel(&mut self, event: Event) {
        let generation = &mut self.generations[event as usize];
        *generation = generation
            .checked_add(1)
            .expect("event generation overflow");
    }

    pub fn pop_event_with_elapsed(&mut self) -> Option<(Event, SystemCycle)> {
        self.discard_stale();

        if self.events.peek()?.0.deadline > self.now {
            return None;
        }

        self.events
            .pop()
            .map(|Reverse(entry)| (entry.event, self.now - entry.scheduled_at))
    }

    fn discard_stale(&mut self) {
        while let Some(Reverse(entry)) = self.events.peek() {
            if entry.generation == self.generations[entry.event as usize] {
                break;
            }

            self.events.pop();
        }
    }
}
