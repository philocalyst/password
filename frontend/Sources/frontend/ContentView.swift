import AppKit
import SwiftCrossUI

/*
TODO:
- [ ] search bar
- [ ] plus button instead of add button
- [ ] filtering and sorting
- [ ] hovering effects
- [ ] hover over and copy prompt on field
- [ ] add ui icons
- [ ] panel to the left of the sidebar for separate stores
- [ ] history drop down
        - hover over each marker in history and shows overlay of what the password looks like at that point
*/

// MARK: - Root

struct ContentView: View {
    @Environment(PasswordStoreViewModel.self) var vm
    @State var showAddSheet = false
    @State var showShareSheet = false
    @State var receiveTicket = ""

    var body: some View {
        NavigationSplitView(
            sidebar: {
                VStack {
                    if vm.entries.isEmpty {
                        VStack {
                            Spacer()
                            Text("No entries")
                                .foregroundColor(.gray)
                            Spacer()
                        }
                    } else {
                        List(vm.entries, id: \.self, selection: vm.$selectedEntry) { name in
                            Text(name)
                        }
                        .onChange(of: vm.selectedEntry) {
                            if let name = vm.selectedEntry { vm.select(name) }
                        }
                    }

                    Divider()

                    HStack {
                        Button("Add") {
                            showAddSheet = true
                        }
                        .foregroundColor(.blue)
                        Button("Delete") {
                            if let name = vm.selectedEntry { vm.remove(name: name) }
                        }
                        .foregroundColor(.red)
                        .disabled(vm.selectedEntry == nil)
                        Spacer()
                        Button("Share") {
                            vm.share()
                            showShareSheet = true
                        }
                    }
                    .padding(8)
                }
                .padding(8)
            },
            detail: {
                if let item = vm.selectedItem, let name = vm.selectedEntry {
                    DetailView(name: name, item: item)
                } else {
                    VStack {
                        Spacer()
                        Text("Select an entry")
                            .foregroundColor(.gray)
                        Spacer()
                    }
                }
            }
        )
        .sheet(isPresented: $showAddSheet) {
            AddEntrySheet(isPresented: $showAddSheet)
                .environment(vm)
        }
        .sheet(isPresented: $showShareSheet) {
            ShareSheet(receiveTicket: $receiveTicket, isPresented: $showShareSheet)
                .environment(vm)
        }
        .alert(vm.$errorMessage) {
            Button("OK") { vm.errorMessage = nil }
        }
    }
}

// MARK: - Detail

struct DetailView: View {
    let name: String
    let item: FfiItem

    @Environment(PasswordStoreViewModel.self) var vm
    @State var showPassword = false
    @State var showHistory = false

    var body: some View {
        ScrollView {
            VStack {
                HStack {
                    Text(name).font(.title).emphasized()
                    Spacer()
                    Text(item.displayName)
                        .foregroundColor(.gray)
                        .font(.caption)
                    Button(showHistory ? "Hide History" : "History") {
                        showHistory.toggle()
                    }
                }
                .padding(.bottom, 8)

                Divider()

                switch item {
                case .onlineAccount(let a):
                    OnlineAccountDetail(name: name, account: a, showPassword: $showPassword)
                case .socialSecurity(let s):
                    SsnDetail(ssn: s)
                }

                if showHistory {
                    Divider().padding(.top, 8)
                    HistoryPanel(name: name)
                        .environment(vm)
                }

                Spacer()
            }
            .padding()
        }
        .frame(minWidth: 420)
    }
}

struct OnlineAccountDetail: View {
    let name: String
    let account: FfiOnlineAccount
    @Binding var showPassword: Bool

    @Environment(PasswordStoreViewModel.self) var vm
    @State var editing = false
    @State var draft = FfiOnlineAccount.empty()

    var body: some View {
        VStack {
            if editing {
                EditOnlineAccountView(name: name, draft: $draft, editing: $editing)
                    .environment(vm)
            } else {
                if let v = account.username { FieldRow(label: "Username", value: v) }
                if let v = account.email { FieldRow(label: "Email", value: v) }
                if let v = account.phone { FieldRow(label: "Phone", value: v) }
                if let v = account.hostWebsite { FieldRow(label: "Website", value: v) }
                if let v = account.password {
                    PasswordRow(password: v, showPassword: $showPassword)
                }
                if let v = account.status { FieldRow(label: "Status", value: v) }
                if let tfa = account.twoFactorEnabled {
                    FieldRow(label: "2FA", value: tfa ? "Enabled" : "Disabled")
                }
                if let v = account.dateCreated { FieldRow(label: "Created", value: v) }
                if let v = account.notes { NotesRow(notes: v) }

                Button("Edit") {
                    draft = account
                    editing = true
                }
                .padding(.top, 8)
            }
        }
    }
}

struct EditOnlineAccountView: View {
    let name: String
    @Binding var draft: FfiOnlineAccount
    @Binding var editing: Bool

    @Environment(PasswordStoreViewModel.self) var vm
    @State var username = ""
    @State var password = ""
    @State var email = ""
    @State var website = ""
    @State var notes = ""

    var body: some View {
        VStack {
            HStack {
                Text("Username").frame(width: 80)
                TextField("", text: $username)
            }
            HStack {
                Text("Password").frame(width: 80)
                TextField("", text: $password)
            }
            HStack {
                Text("Email").frame(width: 80)
                TextField("", text: $email)
            }
            HStack {
                Text("Website").frame(width: 80)
                TextField("", text: $website)
            }
            HStack {
                Text("Notes").frame(width: 80)
                TextField("", text: $notes)
            }

            HStack {
                Button("Cancel") { editing = false }
                Spacer()
                Button("Save") {
                    let updated = FfiOnlineAccount(
                        username: username.isEmpty ? nil : username,
                        password: password.isEmpty ? nil : password,
                        email: email.isEmpty ? nil : email,
                        phone: draft.phone,
                        signInWith: draft.signInWith,
                        status: draft.status,
                        hostWebsite: website.isEmpty ? nil : website,
                        loginPages: draft.loginPages,
                        securityQuestions: draft.securityQuestions,
                        twoFactorEnabled: draft.twoFactorEnabled,
                        associatedItems: draft.associatedItems,
                        dateCreated: draft.dateCreated,
                        notes: notes.isEmpty ? nil : notes
                    )
                    vm.update(name: name, item: .onlineAccount(account: updated))
                    editing = false
                }
            }
            .padding(.top, 8)
        }
        .onAppear {
            username = draft.username ?? ""
            password = draft.password ?? ""
            email = draft.email ?? ""
            website = draft.hostWebsite ?? ""
            notes = draft.notes ?? ""
        }
    }
}

struct SsnDetail: View {
    let ssn: FfiSocialSecurity

    var body: some View {
        VStack {
            FieldRow(label: "Number", value: ssn.accountNumber)
            if let v = ssn.legalName { FieldRow(label: "Name", value: v) }
            if let v = ssn.countryOfIssue { FieldRow(label: "Country", value: v) }
            if let v = ssn.issuanceDate { FieldRow(label: "Issued", value: v) }
            if let v = ssn.notes { NotesRow(notes: v) }
        }
    }
}

// MARK: - Version history panel

struct HistoryPanel: View {
    let name: String
    @Environment(PasswordStoreViewModel.self) var vm

    var body: some View {
        let log = vm.logHistory(for: name)
        VStack {
            HStack {
                Text("History")
                    .font(.caption)
                    .foregroundColor(.gray)
                Spacer()
            }
            if log.isEmpty {
                Text("No history recorded.").font(.caption).foregroundColor(.gray)
            } else {
                ForEach(log, id: \.hash) { entry in
                    VStack {
                        HStack {
                            VStack {
                                HStack {
                                    Text(entry.message).font(.caption)
                                    Spacer()
                                }
                                HStack {
                                    Text(String(entry.hash.prefix(12)))
                                        .font(.caption)
                                        .foregroundColor(.gray)
                                    Text(entry.timestamp)
                                        .font(.caption)
                                        .foregroundColor(.gray)
                                    Spacer()
                                }
                            }
                            Button("Revert") {
                                vm.revert(name: name, toHash: entry.hash)
                            }
                        }
                        .padding(.vertical, 2)
                        Divider()
                    }
                }
            }
        }
    }
}

// MARK: - Field rows

struct FieldRow: View {
    let label: String
    let value: String

    var body: some View {
        HStack {
            Text(label)
                .foregroundColor(.gray)
                .font(.caption)
                .frame(width: 80)
            Text(value)
            Spacer()
            Button("Copy") { copyToClipboard(value) }
        }
        .padding(.vertical, 4)
    }
}

struct PasswordRow: View {
    let password: String
    @Binding var showPassword: Bool

    var body: some View {
        HStack {
            Text("Password")
                .foregroundColor(.gray)
                .font(.caption)
                .frame(width: 80)
            Text(showPassword ? password : String(repeating: "•", count: 16))
            Spacer()
            Button(showPassword ? "Hide" : "Show") { showPassword.toggle() }
            Button("Copy") { copyToClipboard(password) }
        }
        .padding(.vertical, 4)
    }
}

struct NotesRow: View {
    let notes: String

    var body: some View {
        VStack {
            HStack {
                Text("Notes").foregroundColor(.gray).font(.caption)
                Spacer()
            }
            HStack {
                Text(notes)
                Spacer()
            }
            .padding(8)
        }
        .padding(.vertical, 4)
    }
}

// MARK: - Add entry sheet

struct AddEntrySheet: View {
    @Binding var isPresented: Bool

    @Environment(PasswordStoreViewModel.self) var vm
    @State var name = ""
    @State var username = ""
    @State var password = ""
    @State var email = ""
    @State var website = ""
    @State var notes = ""

    var body: some View {
        VStack {
            Text("Add Entry").font(.title).padding(.bottom)

            HStack {
                Text("Name").frame(width: 80)
                TextField("entry-name", text: $name)
            }
            HStack {
                Text("Username").frame(width: 80)
                TextField("", text: $username)
            }
            HStack {
                Text("Password").frame(width: 80)
                TextField("", text: $password)
            }
            HStack {
                Text("Email").frame(width: 80)
                TextField("", text: $email)
            }
            HStack {
                Text("Website").frame(width: 80)
                TextField("https://…", text: $website)
            }
            HStack {
                Text("Notes").frame(width: 80)
                TextField("", text: $notes)
            }

            HStack {
                Button("Cancel") { isPresented = false }
                Spacer()
                Button("Add") {
                    guard !name.isEmpty else { return }
                    let account = FfiOnlineAccount(
                        username: username.isEmpty ? nil : username,
                        password: password.isEmpty ? nil : password,
                        email: email.isEmpty ? nil : email,
                        phone: nil,
                        signInWith: nil,
                        status: "Active",
                        hostWebsite: website.isEmpty ? nil : website,
                        loginPages: nil,
                        securityQuestions: nil,
                        twoFactorEnabled: nil,
                        associatedItems: nil,
                        dateCreated: nil,
                        notes: notes.isEmpty ? nil : notes
                    )
                    vm.add(name: name, item: .onlineAccount(account: account))
                    isPresented = false
                }
                .disabled(name.isEmpty)
            }
            .padding(.top)
        }
        .padding()
        .frame(minWidth: 400)
    }
}

// MARK: - Share / receive sheet

struct ShareSheet: View {
    @Binding var receiveTicket: String
    @Binding var isPresented: Bool

    @Environment(PasswordStoreViewModel.self) var vm

    var body: some View {
        VStack {
            Text("Share / Receive").font(.title).padding(.bottom)

            if let ticket = vm.shareTicket {
                VStack {
                    Text("Share ticket (send this to the receiver):").font(.caption)
                    Text(ticket)
                        .font(.caption)
                        .padding(8)
                    Button("Copy ticket") { copyToClipboard(ticket) }
                }
                .padding(.bottom)
            }

            Divider()

            VStack {
                Text("Paste a ticket to receive:").font(.caption)
                TextField("ticket…", text: $receiveTicket)
                Button("Receive") {
                    vm.receive(ticket: receiveTicket)
                    isPresented = false
                }
                .disabled(receiveTicket.isEmpty)
            }

            Button("Close") { isPresented = false }
                .padding(.top)
        }
        .padding()
        .frame(minWidth: 480)
    }
}

// MARK: - Clipboard

private func copyToClipboard(_ text: String) {
    NSPasteboard.general.clearContents()
    NSPasteboard.general.setString(text, forType: .string)
}
