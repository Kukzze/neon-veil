//! Data-driven behavior trees.
//!
//! A tree is plain data generic over its leaf type, ticked with a closure
//! that evaluates leaves. Trees carry their own runtime state (cooldowns),
//! need no world access and are re-evaluated from the root every tick
//! (reactive), so higher-priority branches interrupt running ones.

use bevy::prelude::*;
use std::time::Duration;

/// Result of ticking a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Reflect)]
#[reflect(Debug, PartialEq, Hash)]
pub enum BehaviorStatus {
    Success,
    Failure,
    Running,
}

/// A behavior tree node with leaves of type `A`.
#[derive(Debug, Clone, PartialEq)]
pub enum BehaviorNode<A> {
    /// First child that does not fail decides; fails if all fail.
    Selector(Vec<BehaviorNode<A>>),
    /// First child that does not succeed decides; succeeds if all succeed.
    Sequence(Vec<BehaviorNode<A>>),
    /// Swaps success and failure; running passes through.
    Inverter(Box<BehaviorNode<A>>),
    /// Fails while cooling down; the cooldown starts when the child succeeds.
    Cooldown {
        timer: Timer,
        child: Box<BehaviorNode<A>>,
    },
    Leaf(A),
}

impl<A> BehaviorNode<A> {
    pub fn selector(children: impl IntoIterator<Item = BehaviorNode<A>>) -> Self {
        Self::Selector(children.into_iter().collect())
    }

    pub fn sequence(children: impl IntoIterator<Item = BehaviorNode<A>>) -> Self {
        Self::Sequence(children.into_iter().collect())
    }

    pub fn inverter(child: BehaviorNode<A>) -> Self {
        Self::Inverter(Box::new(child))
    }

    /// A cooldown that starts ready.
    pub fn cooldown(seconds: f32, child: BehaviorNode<A>) -> Self {
        let mut timer = Timer::from_seconds(seconds.max(0.0), TimerMode::Once);
        timer.tick(timer.duration());
        Self::Cooldown {
            timer,
            child: Box::new(child),
        }
    }

    pub fn leaf(action: A) -> Self {
        Self::Leaf(action)
    }

    /// Advances every cooldown in the tree, whether or not its branch is
    /// evaluated this tick.
    pub fn advance_timers(&mut self, dt: f32) {
        let delta = Duration::from_secs_f32(dt.max(0.0));
        self.advance(delta);
    }

    fn advance(&mut self, delta: Duration) {
        match self {
            Self::Selector(children) | Self::Sequence(children) => {
                for child in children {
                    child.advance(delta);
                }
            }
            Self::Inverter(child) => child.advance(delta),
            Self::Cooldown { timer, child } => {
                timer.tick(delta);
                child.advance(delta);
            }
            Self::Leaf(_) => {}
        }
    }

    /// Evaluates the node, calling `eval` for each leaf reached.
    pub fn tick(&mut self, eval: &mut impl FnMut(&A) -> BehaviorStatus) -> BehaviorStatus {
        match self {
            Self::Selector(children) => {
                for child in children {
                    match child.tick(eval) {
                        BehaviorStatus::Failure => continue,
                        decided => return decided,
                    }
                }
                BehaviorStatus::Failure
            }
            Self::Sequence(children) => {
                for child in children {
                    match child.tick(eval) {
                        BehaviorStatus::Success => continue,
                        decided => return decided,
                    }
                }
                BehaviorStatus::Success
            }
            Self::Inverter(child) => match child.tick(eval) {
                BehaviorStatus::Success => BehaviorStatus::Failure,
                BehaviorStatus::Failure => BehaviorStatus::Success,
                BehaviorStatus::Running => BehaviorStatus::Running,
            },
            Self::Cooldown { timer, child } => {
                if !timer.finished() {
                    return BehaviorStatus::Failure;
                }
                let status = child.tick(eval);
                if status == BehaviorStatus::Success {
                    timer.reset();
                }
                status
            }
            Self::Leaf(action) => eval(action),
        }
    }
}

/// A tree plus its last result.
#[derive(Debug, Clone, PartialEq)]
pub struct BehaviorTree<A> {
    pub root: BehaviorNode<A>,
    pub last_status: Option<BehaviorStatus>,
}

impl<A> BehaviorTree<A> {
    pub fn new(root: BehaviorNode<A>) -> Self {
        Self {
            root,
            last_status: None,
        }
    }

    /// Advances cooldowns by `dt`, then evaluates from the root.
    pub fn tick(&mut self, dt: f32, eval: &mut impl FnMut(&A) -> BehaviorStatus) -> BehaviorStatus {
        self.root.advance_timers(dt);
        let status = self.root.tick(eval);
        self.last_status = Some(status);
        status
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use BehaviorStatus::*;

    /// Leaves return a fixed status and record that they ran.
    #[derive(Debug, Clone, Copy, PartialEq)]
    struct Fixed(&'static str, BehaviorStatus);

    fn run(node: &mut BehaviorNode<Fixed>) -> (BehaviorStatus, Vec<&'static str>) {
        let mut visited = Vec::new();
        let status = node.tick(&mut |leaf: &Fixed| {
            visited.push(leaf.0);
            leaf.1
        });
        (status, visited)
    }

    fn leaf(name: &'static str, status: BehaviorStatus) -> BehaviorNode<Fixed> {
        BehaviorNode::leaf(Fixed(name, status))
    }

    #[test]
    fn selector_succeeds_on_second_child_after_first_fails() {
        let mut node =
            BehaviorNode::selector([leaf("a", Failure), leaf("b", Success), leaf("c", Success)]);
        assert_eq!(
            run(&mut node),
            (Success, vec!["a", "b"]),
            "stops at the first success"
        );
    }

    #[test]
    fn selector_returns_running_and_fails_when_all_fail() {
        let mut running =
            BehaviorNode::selector([leaf("a", Failure), leaf("b", Running), leaf("c", Success)]);
        assert_eq!(run(&mut running), (Running, vec!["a", "b"]));
        let mut all_fail = BehaviorNode::selector([leaf("a", Failure), leaf("b", Failure)]);
        assert_eq!(run(&mut all_fail).0, Failure);
        let mut empty = BehaviorNode::<Fixed>::selector([]);
        assert_eq!(run(&mut empty).0, Failure);
    }

    #[test]
    fn sequence_fails_on_second_child_after_first_succeeds() {
        let mut node =
            BehaviorNode::sequence([leaf("a", Success), leaf("b", Failure), leaf("c", Success)]);
        assert_eq!(
            run(&mut node),
            (Failure, vec!["a", "b"]),
            "stops at the first failure"
        );
    }

    #[test]
    fn sequence_returns_running_and_succeeds_when_all_succeed() {
        let mut running =
            BehaviorNode::sequence([leaf("a", Success), leaf("b", Running), leaf("c", Failure)]);
        assert_eq!(run(&mut running), (Running, vec!["a", "b"]));
        let mut all = BehaviorNode::sequence([leaf("a", Success), leaf("b", Success)]);
        assert_eq!(run(&mut all), (Success, vec!["a", "b"]));
        let mut empty = BehaviorNode::<Fixed>::sequence([]);
        assert_eq!(run(&mut empty).0, Success);
    }

    #[test]
    fn inverter_flips_success_and_failure() {
        assert_eq!(
            run(&mut BehaviorNode::inverter(leaf("a", Success))).0,
            Failure
        );
        assert_eq!(
            run(&mut BehaviorNode::inverter(leaf("a", Failure))).0,
            Success
        );
        assert_eq!(
            run(&mut BehaviorNode::inverter(leaf("a", Running))).0,
            Running
        );
    }

    #[test]
    fn cooldown_blocks_until_its_timer_expires() {
        let mut tree = BehaviorTree::new(BehaviorNode::cooldown(1.0, leaf("fire", Success)));
        let mut fired = 0;
        let mut eval = |leaf: &Fixed| {
            fired += 1;
            leaf.1
        };
        assert_eq!(tree.tick(0.0, &mut eval), Success, "starts ready");
        for _ in 0..63 {
            assert_eq!(tree.tick(1.0 / 64.0, &mut eval), Failure, "cooling down");
        }
        assert_eq!(tree.tick(1.0 / 64.0, &mut eval), Success, "ready after 1 s");
        assert_eq!(fired, 2, "the child only ran when allowed");
    }

    #[test]
    fn cooldown_only_starts_on_success() {
        let mut tree = BehaviorTree::new(BehaviorNode::cooldown(5.0, leaf("aim", Running)));
        let mut eval = |leaf: &Fixed| leaf.1;
        assert_eq!(tree.tick(0.1, &mut eval), Running);
        assert_eq!(
            tree.tick(0.1, &mut eval),
            Running,
            "running does not consume the cooldown"
        );
    }

    #[test]
    fn unevaluated_cooldowns_still_recover() {
        // The cooldown sits behind a gate; while the gate succeeds the
        // cooldown branch is never reached, yet it must keep counting down.
        let mut tree = BehaviorTree::new(BehaviorNode::selector([
            leaf("gate", Success),
            BehaviorNode::cooldown(1.0, leaf("special", Success)),
        ]));
        let tick = |tree: &mut BehaviorTree<Fixed>, dt: f32, gate: BehaviorStatus| {
            let mut visited = Vec::new();
            tree.tick(dt, &mut |leaf: &Fixed| {
                visited.push(leaf.0);
                if leaf.0 == "gate" {
                    gate
                } else {
                    leaf.1
                }
            });
            visited
        };
        assert_eq!(
            tick(&mut tree, 0.0, Failure),
            vec!["gate", "special"],
            "special fires"
        );
        assert_eq!(
            tick(&mut tree, 1.0, Success),
            vec!["gate"],
            "cooldown branch skipped"
        );
        assert_eq!(
            tick(&mut tree, 0.0, Failure),
            vec!["gate", "special"],
            "recovered while skipped"
        );
        assert_eq!(tree.last_status, Some(Success));
    }
}
