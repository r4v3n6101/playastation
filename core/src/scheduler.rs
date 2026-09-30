use alloc::collections::BinaryHeap;
use core::cmp::Reverse;

use strum::EnumCount;

/// This is CPU time in cycles.
pub type SystemCycle = u64;

#[derive(Debug, Clone, Default)]
pub struct Scheduler {
    now: SystemCycle,
    events: BinaryHeap<Reverse<QueuedEvent>>,
    generations: [u64; Event::COUNT],
}

/// One pending wakeup per variant. Devices keep their internal task queues.
/// Declaration order breaks ties between deadlines; it is not hardware priority.
#[derive(EnumCount, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Event {
    Gpu,
    Timer0,
    Timer1,
    Timer2,
    CdRom,
    Joy,
    Spu,
    Dma,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct QueuedEvent {
    deadline: SystemCycle,
    event: Event,
    generation: u64,
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
        }));
    }

    pub fn cancel(&mut self, event: Event) {
        let generation = &mut self.generations[event as usize];
        *generation = generation
            .checked_add(1)
            .expect("event generation overflow");
    }

    /// Remove the earliest due event without advancing time.
    /// The handler may schedule further events before the next pop.
    pub fn pop(&mut self) -> Option<Event> {
        self.discard_stale();

        if self.events.peek()?.0.deadline > self.now {
            return None;
        }

        self.events.pop().map(|entry| entry.0.event)
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
