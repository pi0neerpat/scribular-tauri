public enum PickResult<T> {
    case found(T)
    case none
    case ambiguous(count: Int)
}

extension PickResult: Equatable where T: Equatable {}

public func pick<T>(_ matches: [T], index: Int?) -> PickResult<T> {
    if let index {
        return matches.indices.contains(index - 1) ? .found(matches[index - 1]) : .none
    }
    switch matches.count {
    case 0: return .none
    case 1: return .found(matches[0])
    case let count: return .ambiguous(count: count)
    }
}
