use forge::util::ForgeError;
use std::io;

#[test]
fn test_forge_error_variants() {
    // Test that we can create ForgeError variants
    let gpu_err = ForgeError::Gpu("Test GPU error".to_string());
    let surface_err = ForgeError::Surface("Test surface error".to_string());
    let command_err = ForgeError::Command("Test command error".to_string());
    let parse_err = ForgeError::Parse("Test parse error".to_string());
    let io_err = ForgeError::Io(io::Error::new(io::ErrorKind::NotFound, "Test IO error"));
    
    // Verify they contain the expected strings
    assert_eq!(format!("{}", gpu_err), "GPU error: Test GPU error");
    assert_eq!(format!("{}", surface_err), "Surface error: Test surface error");
    assert_eq!(format!("{}", command_err), "Command error: Test command error");
    assert_eq!(format!("{}", parse_err), "Parse error: Test parse error");
    // IO error might include OS-specific details, so just check the prefix
    assert!(format!("{}", io_err).starts_with("IO error: Test IO error"));
}