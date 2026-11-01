//! Decision making: player brains (beliefs about phase, block and duties) over the world cache, off-ball
//! positioning, on-ball decisions, personality and execution. Each module names the catalog principles it implements.

pub mod analysis;
pub mod brain;
pub mod deviation;
pub mod execution;
pub mod movement;
pub mod offer;
pub mod on_ball;
pub mod personality;
pub mod positioning;
pub mod reach;
pub mod reading;
pub mod scan;
pub mod set_piece;
pub mod shape;
pub mod world;
