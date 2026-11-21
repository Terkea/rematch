//! Splitting the PGN stream into games (R2 Task 19).
//!
//! Rule: a new game starts at a line beginning `[Event ` that follows a blank line (or opens the
//! stream). Leading blank lines are dropped, `\r\n` is accepted, and each game is returned with
//! its trailing blank lines removed and a single final `\n`.

/// Incremental splitter: feed lines, get each game as soon as the next one starts.
#[derive(Debug)]
pub struct GameSplitter {
    current: String,
    has_content: bool,
    prev_blank: bool,
}

impl Default for GameSplitter {
    fn default() -> Self {
        Self::new()
    }
}

impl GameSplitter {
    pub fn new() -> Self {
        Self {
            current: String::new(),
            has_content: false,
            prev_blank: true,
        }
    }

    /// Feeds one line (with or without its `\n` / `\r\n`). Returns the previous game when this
    /// line starts a new one.
    pub fn push_line(&mut self, line: &str) -> Option<String> {
        let line = line.strip_suffix('\n').unwrap_or(line);
        let line = line.strip_suffix('\r').unwrap_or(line);
        let blank = line.trim().is_empty();
        let done = if self.prev_blank && self.has_content && line.starts_with("[Event ") {
            Some(self.take())
        } else {
            None
        };
        if !blank || self.has_content {
            self.current.push_str(line);
            self.current.push('\n');
            self.has_content |= !blank;
        }
        self.prev_blank = blank;
        done
    }

    /// The last game at the end of the stream, if any.
    pub fn finish(&mut self) -> Option<String> {
        self.prev_blank = true;
        self.has_content.then(|| self.take())
    }

    /// Bytes buffered for the game in progress.
    pub fn buffered_len(&self) -> usize {
        self.current.len()
    }

    fn take(&mut self) -> String {
        let mut game = std::mem::take(&mut self.current);
        let len = game.trim_end().len();
        game.truncate(len);
        game.push('\n');
        self.has_content = false;
        game
    }
}

/// Splits a sequence of lines into games (see the module rule).
pub fn split_games<I, S>(lines: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut splitter = GameSplitter::new();
    let mut games: Vec<String> = lines
        .into_iter()
        .filter_map(|l| splitter.push_line(l.as_ref()))
        .collect();
    games.extend(splitter.finish());
    games
}
