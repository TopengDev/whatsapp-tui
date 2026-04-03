/// Every discrete action the user can trigger.
/// Keybindings map to these. Commands (`:quit`) also resolve to these.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    // Navigation
    NextItem,
    PrevItem,
    FirstItem,
    LastItem,
    HalfPageDown,
    HalfPageUp,
    OpenChat,
    CloseChat,

    // Focus
    CycleFocus,
    FocusLeft,
    FocusRight,
    ToggleInfoPanel,

    // Mode transitions
    EnterInsert,
    ExitInsert,
    EnterCommand,
    EnterSearch,
    ExitOverlay,

    // Messaging
    SendMessage,
    ReplyToSelected,
    EditSelected,
    DeleteSelected,
    ReactToSelected,
    YankSelected,
    OpenMedia,
    DownloadMedia,

    // Chat management
    MuteChat,
    ArchiveChat,
    PinChat,

    // Search / Command
    SubmitCommand(String),
    SubmitSearch(String),
    SearchNext,
    SearchPrev,

    // App
    Quit,
    Confirm,
    Cancel,
}
