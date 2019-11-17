use twiggy_traits as traits;

pub mod item_name;
pub mod location_attrs;

/// This type alias is used to represent an option return value for
/// a procedure that could return an Error.
type FallilbleOption<T> = Result<Option<T>, traits::Error>;
