//! `BinaryHeap` (`packages/common/src/binary-heap.ts`): the min-heap the
//! elbow arrow router keeps its open set in.
//!
//! The order items come out in when scores tie is part of the route
//! upstream draws, so this is a line-for-line port rather than
//! `std::collections::BinaryHeap`: the same sift-up (`sinkDown`) and
//! sift-down (`bubbleUp`) steps, strict `<` comparisons, and
//! `rescoreElement` finding the item by a linear search.

/// A min-heap of `T` ordered by a score read at the time of each
/// comparison. Upstream's heap holds its `scoreFunction`; here each
/// operation is handed it, so the scores can live in the router's grid
/// while the grid is being updated between operations.
pub(crate) struct BinaryHeap<T> {
    content: Vec<T>,
}

impl<T: Copy + PartialEq> BinaryHeap<T> {
    /// `new BinaryHeap(scoreFunction)`.
    pub(crate) fn new() -> BinaryHeap<T> {
        BinaryHeap {
            content: Vec::new(),
        }
    }

    /// `sinkDown(idx)`: moves the item at `idx` towards the root while it
    /// scores lower than its parent.
    fn sink_down(&mut self, mut idx: usize, score: &impl Fn(T) -> f64) {
        let node = self.content[idx];
        let node_score = score(node);
        while idx > 0 {
            let parent_n = ((idx + 1) >> 1) - 1;
            let parent = self.content[parent_n];
            if node_score < score(parent) {
                self.content[idx] = parent;
                idx = parent_n;
            } else {
                break;
            }
        }
        self.content[idx] = node;
    }

    /// `bubbleUp(idx)`: moves the item at `idx` towards the leaves while a
    /// child scores lower, the left child first.
    fn bubble_up(&mut self, mut idx: usize, score_of: &impl Fn(T) -> f64) {
        let length = self.content.len();
        let node = self.content[idx];
        let score = score_of(node);
        loop {
            let child1_n = ((idx + 1) << 1) - 1;
            let child2_n = child1_n + 1;
            let mut smallest_idx = idx;
            let mut smallest_score = score;
            if child1_n < length {
                let child1_score = score_of(self.content[child1_n]);
                if child1_score < smallest_score {
                    smallest_idx = child1_n;
                    smallest_score = child1_score;
                }
            }
            if child2_n < length {
                let child2_score = score_of(self.content[child2_n]);
                if child2_score < smallest_score {
                    smallest_idx = child2_n;
                }
            }
            if smallest_idx == idx {
                break;
            }
            self.content[idx] = self.content[smallest_idx];
            idx = smallest_idx;
        }
        self.content[idx] = node;
    }

    /// `push(node)`.
    pub(crate) fn push(&mut self, node: T, score: &impl Fn(T) -> f64) {
        self.content.push(node);
        self.sink_down(self.content.len() - 1, score);
    }

    /// `pop()`: the lowest-scoring item, `None` when empty.
    pub(crate) fn pop(&mut self, score: &impl Fn(T) -> f64) -> Option<T> {
        let end = self.content.pop()?;
        if self.content.is_empty() {
            return Some(end);
        }
        let result = self.content[0];
        self.content[0] = end;
        self.bubble_up(0, score);
        Some(result)
    }

    /// `size()`.
    pub(crate) fn size(&self) -> usize {
        self.content.len()
    }

    /// `rescoreElement(node)`: after `node`'s score dropped, moves it
    /// towards the root. Upstream calls it only for items in the heap; an
    /// item that is not there is left alone.
    pub(crate) fn rescore_element(&mut self, node: T, score: &impl Fn(T) -> f64) {
        if let Some(idx) = self.content.iter().position(|&n| n == node) {
            self.sink_down(idx, score);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::BinaryHeap;

    #[test]
    fn pops_in_score_order() {
        let scores = [5.0, 1.0, 4.0, 1.0, 3.0, 9.0, 0.5];
        let score = |i: usize| scores[i];
        let mut heap = BinaryHeap::new();
        for i in 0..scores.len() {
            heap.push(i, &score);
        }
        assert_eq!(heap.size(), 7);
        let mut out = Vec::new();
        while let Some(i) = heap.pop(&score) {
            out.push(i);
        }
        // ties come out in heap order, not insertion order: upstream's
        // BinaryHeap pops these as [6, 3, 1, 4, 2, 0, 5] (run under node
        // from the pinned checkout)
        assert_eq!(out, [6, 3, 1, 4, 2, 0, 5]);
        assert_eq!(heap.pop(&score), None);
    }

    #[test]
    fn rescore_moves_an_item_up() {
        let mut scores = [1.0, 2.0, 3.0, 4.0];
        let mut heap = BinaryHeap::new();
        for i in 0..4 {
            heap.push(i, &|i: usize| scores[i]);
        }
        scores[3] = 0.0;
        let score = |i: usize| scores[i];
        heap.rescore_element(3, &score);
        assert_eq!(heap.pop(&score), Some(3));
        assert_eq!(heap.pop(&score), Some(0));
    }
}
