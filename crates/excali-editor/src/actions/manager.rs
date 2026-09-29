//! `ActionManager` (`actions/manager.tsx`): which registered action a key
//! runs, whether an action may run or render, and what analytics it
//! reports. Running `perform` and applying its result is the caller's.

use super::context::{ActionContext, FormFactor};
use super::keys::KeyEvent;
use super::names::ActionName;
use super::registry::ActionSpec;

/// Where an action was run from (`ActionSource`, `types.ts:17-22`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionSource {
    Ui,
    Keyboard,
    ContextMenu,
    Api,
    CommandPalette,
}

impl ActionSource {
    /// The string upstream uses.
    pub fn as_str(self) -> &'static str {
        match self {
            ActionSource::Ui => "ui",
            ActionSource::Keyboard => "keyboard",
            ActionSource::ContextMenu => "contextMenu",
            ActionSource::Api => "api",
            ActionSource::CommandPalette => "commandPalette",
        }
    }
}

/// What `handleKeyDown` did with a keydown (`manager.tsx:92-148`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyDownOutcome {
    /// No action takes the key (returns `false`).
    Unhandled,
    /// More than one action's `keyTest` passed; upstream warns and does
    /// nothing (returns `false`). In `keyPriority` order.
    Ambiguous(Vec<ActionName>),
    /// A navigation action matched while a locked viewport transition is
    /// pending: the event is prevented and consumed, nothing runs.
    Swallowed(ActionName),
    /// Prevent the event and run the action with source `"keyboard"`.
    Perform(ActionName),
}

/// One analytics event (`trackAction`, `manager.tsx:22-51`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackedEvent {
    pub category: &'static str,
    pub action: String,
    /// `"<source> (mobile|desktop)"`.
    pub label: String,
}

/// The actions registered with the editor, by name.
#[derive(Debug, Clone)]
pub struct ActionManager {
    registered: Vec<ActionName>,
}

impl Default for ActionManager {
    fn default() -> ActionManager {
        ActionManager::new()
    }
}

impl ActionManager {
    /// The manager as `App` builds it (`App.tsx:944-946`): every
    /// registered action plus undo and redo.
    pub fn new() -> ActionManager {
        ActionManager {
            registered: ActionName::ALL
                .into_iter()
                .filter(|a| a.spec().is_registered())
                .collect(),
        }
    }

    /// A manager with nothing registered.
    pub fn empty() -> ActionManager {
        ActionManager {
            registered: Vec::new(),
        }
    }

    /// `registerAction`: `actions[name] = action`.
    pub fn register(&mut self, name: ActionName) {
        if !self.registered.contains(&name) {
            self.registered.push(name);
        }
    }

    pub fn is_registered(&self, name: ActionName) -> bool {
        self.registered.contains(&name)
    }

    /// The registered actions, in registration order.
    pub fn registered(&self) -> impl Iterator<Item = &'static ActionSpec> + '_ {
        self.registered.iter().map(|a| a.spec())
    }

    /// `handleKeyDown(event)` (`manager.tsx:92-148`).
    pub fn handle_key_down(&self, event: &KeyEvent<'_>, ctx: &ActionContext<'_>) -> KeyDownOutcome {
        let env = ctx.env;
        if !env.interaction_enabled && !env.navigation_enabled {
            return KeyDownOutcome::Unhandled;
        }
        let mut candidates: Vec<&'static ActionSpec> = self.registered().collect();
        // Array.prototype.sort is stable: equal priorities keep their order.
        candidates.sort_by_key(|a| std::cmp::Reverse(a.key_priority));
        let matched: Vec<&'static ActionSpec> = candidates
            .into_iter()
            .filter(|a| {
                ctx.props.canvas_actions.allows(a.name.as_str())
                    && a.key_test.is_some_and(|test| test(event, ctx))
            })
            .collect();
        let action = match matched.as_slice() {
            [] => return KeyDownOutcome::Unhandled,
            [one] => *one,
            many => return KeyDownOutcome::Ambiguous(many.iter().map(|a| a.name).collect()),
        };
        if !env.interaction_enabled && !action.navigation {
            return KeyDownOutcome::Unhandled;
        }
        if ctx.flag("viewModeEnabled") && action.view_mode != Some(true) {
            return KeyDownOutcome::Unhandled;
        }
        if action.navigation && env.viewport_transition_pending {
            return KeyDownOutcome::Swallowed(action.name);
        }
        KeyDownOutcome::Perform(action.name)
    }

    /// Whether `executeAction(action, source)` runs `perform`
    /// (`manager.tsx:150-176`): outside the interactive editor only the
    /// host (`"api"`) and navigation actions may, and nothing navigates
    /// during a locked viewport transition.
    pub fn can_execute(
        &self,
        name: ActionName,
        source: ActionSource,
        ctx: &ActionContext<'_>,
    ) -> bool {
        let spec = name.spec();
        let env = ctx.env;
        if source != ActionSource::Api
            && !env.interaction_enabled
            && !(env.navigation_enabled && spec.navigation)
        {
            return false;
        }
        !(spec.navigation && env.viewport_transition_pending)
    }

    /// Whether `renderAction(name)` renders the action's panel
    /// (`manager.tsx:181-230`).
    pub fn can_render(&self, name: ActionName, ctx: &ActionContext<'_>) -> bool {
        self.is_registered(name)
            && name.spec().has_panel
            && ctx.props.canvas_actions.allows(name.as_str())
    }

    /// `isActionEnabled(action)` (`manager.tsx:232-241`): no predicate, or
    /// the predicate holds.
    pub fn is_action_enabled(&self, name: ActionName, ctx: &ActionContext<'_>) -> bool {
        name.spec().predicate.is_none_or(|p| p(ctx))
    }
}

/// `trackAction(action, source, appState, ...)` (`manager.tsx:22-51`): the
/// analytics event, if the action tracks one in this state.
pub fn track_action(
    spec: &ActionSpec,
    source: ActionSource,
    ctx: &ActionContext<'_>,
) -> Option<TrackedEvent> {
    let track = spec.track_event?;
    if let Some(predicate) = track.predicate {
        if !predicate(ctx.app_state) {
            return None;
        }
    }
    let device = if ctx.env.form_factor == FormFactor::Phone {
        "mobile"
    } else {
        "desktop"
    };
    Some(TrackedEvent {
        category: track.category,
        action: track.action.unwrap_or(spec.name.as_str()).to_owned(),
        label: format!("{} ({device})", source.as_str()),
    })
}
