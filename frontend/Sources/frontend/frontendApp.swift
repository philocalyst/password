import DefaultBackend
import SwiftCrossUI

@main
struct frontendApp: App {
    @State var vm = PasswordStoreViewModel()

    var body: some Scene {
        WindowGroup("Password Manager") {
            ContentView()
                .environment(vm)
        }
        .defaultSize(width: 900, height: 600)
    }
}
