import Foundation
import SwiftCrossUI

@ObservableObject
final class PasswordStoreViewModel {
    var entries: [String] = []
    var selectedEntry: String? = nil
    var selectedItem: FfiItem? = nil
    var errorMessage: String? = nil
    var shareTicket: String? = nil

    private let store: PwdStore

    init() {
        let dir = ProcessInfo.processInfo.environment["PASSWORD_STORE_PATH"]
            ?? (FileManager.default.homeDirectoryForCurrentUser.path + "/.pwd")
        store = try! PwdStore.open(storeDir: dir, branch: "main")
        reload()
    }

    func reload() {
        do {
            entries = try store.listEntries()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func select(_ name: String) {
        selectedEntry = name
        do {
            selectedItem = try store.getEntry(name: name)
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func add(name: String, item: FfiItem, message: String = "") {
        do {
            try store.addEntry(name: name, item: item, message: message)
            reload()
            select(name)
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func update(name: String, item: FfiItem, message: String = "") {
        do {
            try store.updateEntry(name: name, item: item, message: message)
            if selectedEntry == name { select(name) }
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func remove(name: String) {
        do {
            _ = try store.removeEntry(name: name, message: "")
            if selectedEntry == name {
                selectedEntry = nil
                selectedItem = nil
            }
            reload()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func logHistory(for name: String? = nil) -> [FfiChangeEntry] {
        (try? store.logHistory(entryFilter: name)) ?? []
    }

    func revert(name: String, toHash: String) {
        do {
            try store.revertEntry(name: name, toHash: toHash)
            if selectedEntry == name { select(name) }
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    // MARK: P2P

    func share() {
        let handle = P2pHandle()
        do {
            shareTicket = try handle.shareStore(store: store)
        } catch {
            errorMessage = error.localizedDescription
        }
        _ = try? handle.shutdown()
    }

    func receive(ticket: String) {
        let handle = P2pHandle()
        do {
            let count = try handle.receiveInto(ticket: ticket, targetStore: store)
            reload()
            errorMessage = "Received \(count) entries."
        } catch {
            errorMessage = error.localizedDescription
        }
        _ = try? handle.shutdown()
    }
}

// MARK: - Convenience extensions on generated types

extension FfiOnlineAccount {
    static func empty() -> FfiOnlineAccount {
        FfiOnlineAccount(
            username: nil, password: nil, email: nil, phone: nil,
            signInWith: nil, status: "Active", hostWebsite: nil,
            loginPages: nil, securityQuestions: nil,
            twoFactorEnabled: nil, associatedItems: nil,
            dateCreated: nil, notes: nil
        )
    }
}

extension FfiItem {
    var displayName: String {
        switch self {
        case .onlineAccount: return "Online Account"
        case .socialSecurity: return "Social Security"
        }
    }

    var onlineAccount: FfiOnlineAccount? {
        if case .onlineAccount(let a) = self { return a }
        return nil
    }

    var socialSecurity: FfiSocialSecurity? {
        if case .socialSecurity(let s) = self { return s }
        return nil
    }
}
