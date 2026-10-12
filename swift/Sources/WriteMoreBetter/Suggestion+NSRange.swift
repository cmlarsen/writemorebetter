import Foundation

extension Suggestion {
    /// The flagged span as an `NSRange`, ready for `NSAttributedString`, `UITextView` and friends.
    public var nsRange: NSRange {
        NSRange(location: Int(start), length: Int(end) - Int(start))
    }
}
