use crossterm::event::{KeyCode, KeyEvent};

/// Generic navigation handler for list-based tabs
/// Returns true if the key was handled, false otherwise
pub fn handle_list_navigation(key: KeyEvent, selected_index: &mut usize, max_index: usize) -> bool {
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            if *selected_index > 0 {
                *selected_index -= 1;
            }
            true
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if *selected_index < max_index {
                *selected_index += 1;
            }
            true
        }
        KeyCode::Home => {
            *selected_index = 0;
            true
        }
        KeyCode::End => {
            *selected_index = max_index;
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn make_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::empty())
    }

    #[test]
    fn test_navigation_up() {
        let mut index = 5;
        assert!(handle_list_navigation(
            make_key(KeyCode::Up),
            &mut index,
            10
        ));
        assert_eq!(index, 4);
    }

    #[test]
    fn test_navigation_down() {
        let mut index = 5;
        assert!(handle_list_navigation(
            make_key(KeyCode::Down),
            &mut index,
            10
        ));
        assert_eq!(index, 6);
    }

    #[test]
    fn test_navigation_home() {
        let mut index = 5;
        assert!(handle_list_navigation(
            make_key(KeyCode::Home),
            &mut index,
            10
        ));
        assert_eq!(index, 0);
    }

    #[test]
    fn test_navigation_end() {
        let mut index = 5;
        assert!(handle_list_navigation(
            make_key(KeyCode::End),
            &mut index,
            10
        ));
        assert_eq!(index, 10);
    }

    #[test]
    fn test_navigation_bounds_up() {
        let mut index = 0;
        assert!(handle_list_navigation(
            make_key(KeyCode::Up),
            &mut index,
            10
        ));
        assert_eq!(index, 0);
    }

    #[test]
    fn test_navigation_bounds_down() {
        let mut index = 10;
        assert!(handle_list_navigation(
            make_key(KeyCode::Down),
            &mut index,
            10
        ));
        assert_eq!(index, 10);
    }

    #[test]
    fn test_unhandled_key() {
        let mut index = 5;
        assert!(!handle_list_navigation(
            make_key(KeyCode::Char('a')),
            &mut index,
            10
        ));
        assert_eq!(index, 5);
    }
}
