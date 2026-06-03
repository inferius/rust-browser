//! DOMException + JavaScript error names per WebIDL.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DomExceptionKind {
    IndexSizeError,            // legacy
    HierarchyRequestError,
    WrongDocumentError,
    InvalidCharacterError,
    NoModificationAllowedError,
    NotFoundError,
    NotSupportedError,
    InUseAttributeError,
    InvalidStateError,
    SyntaxError,
    InvalidModificationError,
    NamespaceError,
    InvalidAccessError,
    SecurityError,
    NetworkError,
    AbortError,
    UrlMismatchError,
    QuotaExceededError,
    TimeoutError,
    InvalidNodeTypeError,
    DataCloneError,
    EncodingError,
    NotReadableError,
    UnknownError,
    ConstraintError,
    DataError,
    TransactionInactiveError,
    ReadOnlyError,
    VersionError,
    OperationError,
    NotAllowedError,
    OptOutError,                // FedCM
}

impl DomExceptionKind {
    pub fn name(&self) -> &'static str {
        match self {
            Self::IndexSizeError => "IndexSizeError",
            Self::HierarchyRequestError => "HierarchyRequestError",
            Self::WrongDocumentError => "WrongDocumentError",
            Self::InvalidCharacterError => "InvalidCharacterError",
            Self::NoModificationAllowedError => "NoModificationAllowedError",
            Self::NotFoundError => "NotFoundError",
            Self::NotSupportedError => "NotSupportedError",
            Self::InUseAttributeError => "InUseAttributeError",
            Self::InvalidStateError => "InvalidStateError",
            Self::SyntaxError => "SyntaxError",
            Self::InvalidModificationError => "InvalidModificationError",
            Self::NamespaceError => "NamespaceError",
            Self::InvalidAccessError => "InvalidAccessError",
            Self::SecurityError => "SecurityError",
            Self::NetworkError => "NetworkError",
            Self::AbortError => "AbortError",
            Self::UrlMismatchError => "URLMismatchError",
            Self::QuotaExceededError => "QuotaExceededError",
            Self::TimeoutError => "TimeoutError",
            Self::InvalidNodeTypeError => "InvalidNodeTypeError",
            Self::DataCloneError => "DataCloneError",
            Self::EncodingError => "EncodingError",
            Self::NotReadableError => "NotReadableError",
            Self::UnknownError => "UnknownError",
            Self::ConstraintError => "ConstraintError",
            Self::DataError => "DataError",
            Self::TransactionInactiveError => "TransactionInactiveError",
            Self::ReadOnlyError => "ReadOnlyError",
            Self::VersionError => "VersionError",
            Self::OperationError => "OperationError",
            Self::NotAllowedError => "NotAllowedError",
            Self::OptOutError => "OptOutError",
        }
    }

    /// Legacy DOMException "code" property mapping (some entries have no code).
    pub fn legacy_code(&self) -> u16 {
        match self {
            Self::IndexSizeError => 1,
            Self::HierarchyRequestError => 3,
            Self::WrongDocumentError => 4,
            Self::InvalidCharacterError => 5,
            Self::NoModificationAllowedError => 7,
            Self::NotFoundError => 8,
            Self::NotSupportedError => 9,
            Self::InUseAttributeError => 10,
            Self::InvalidStateError => 11,
            Self::SyntaxError => 12,
            Self::InvalidModificationError => 13,
            Self::NamespaceError => 14,
            Self::InvalidAccessError => 15,
            _ => 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DomException {
    pub kind: DomExceptionKind,
    pub message: String,
}

impl DomException {
    pub fn new(kind: DomExceptionKind, message: &str) -> Self {
        Self { kind, message: message.into() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_lookup() {
        assert_eq!(DomExceptionKind::NotFoundError.name(), "NotFoundError");
        assert_eq!(DomExceptionKind::AbortError.name(), "AbortError");
    }

    #[test]
    fn legacy_code_mapping() {
        assert_eq!(DomExceptionKind::IndexSizeError.legacy_code(), 1);
        assert_eq!(DomExceptionKind::NotFoundError.legacy_code(), 8);
        assert_eq!(DomExceptionKind::AbortError.legacy_code(), 0);
    }

    #[test]
    fn construct_with_message() {
        let e = DomException::new(DomExceptionKind::SecurityError, "blocked");
        assert_eq!(e.kind, DomExceptionKind::SecurityError);
        assert_eq!(e.message, "blocked");
    }
}
