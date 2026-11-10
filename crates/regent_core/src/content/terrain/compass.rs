//! Which way is north, in GPL and on screen.
//!
//! Two links:
//!
//! | Link | Held by |
//! |---|---|
//! | board coordinates → screen | none: no turn, no mirror (below) |
//! | direction → art slot | `facing` of a 32-point direction, in `regent_draw` |
//!
//! **Game.** `OriginalQuests/GPL/LowLevel.gpl:894` declares the compass and
//! spawns monsters at a map edge with it:
//!
//! ```text
//! expression #North 0   #south 1   #East 2   #West 3
//!
//! if (whichside == #North)  begin x2 = random; y2 = 30;          end
//! if (whichside == #South)  begin x2 = random; y2 = y_size - 30; end
//! if (whichside == #East)   begin x2 = x_size - 30; y2 = random; end
//! if (whichside == #West)   begin x2 = 30;          y2 = random; end
//! ```
//!
//! North is −y, south +y, east +x, west −x; the origin is the north-west
//! corner. `mx_LowLevel.gpl` is the same. In the engine's 32 directions
//! north is 0, east 8, south 16, west 24.
//!
//! **Viewer.** Board `+x` is right, `+y` down, so north is up: the five named
//! buildings of a save rank on screen in their board order; every other
//! [`Turn`](super::schema::Turn) swaps a pair (**Measured**). Art agrees: slot *i* of *n* faces *i*/*n* turn
//! clockwise from up; the SDK's `SetEffectorDirection` numbers 0 as north.
