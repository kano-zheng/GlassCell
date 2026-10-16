import AppKit
import SwiftUI

/// An AppKit pop-up button for the toolbar: it shows its current title whatever the toolbar's
/// display mode (SwiftUI pickers and menus in a toolbar show only their icon).
struct PopUpPicker: NSViewRepresentable {
    let items: [String]
    let selected: String
    /// Fixed width (long titles truncate rather than widen the toolbar).
    var width: CGFloat = 80
    let pick: (String) -> Void

    func makeNSView(context: Context) -> NSPopUpButton {
        let button = NSPopUpButton(frame: NSRect(x: 0, y: 0, width: width, height: 24), pullsDown: false)
        // The toolbar's glass is the bezel.
        button.isBordered = false
        button.controlSize = .regular
        (button.cell as? NSPopUpButtonCell)?.lineBreakMode = .byTruncatingTail
        button.translatesAutoresizingMaskIntoConstraints = false
        button.widthAnchor.constraint(equalToConstant: width).isActive = true
        button.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        button.target = context.coordinator
        button.action = #selector(Coordinator.changed(_:))
        return button
    }

    func updateNSView(_ button: NSPopUpButton, context: Context) {
        context.coordinator.pick = pick
        if button.itemTitles != items {
            button.removeAllItems()
            button.addItems(withTitles: items)
        }
        if button.titleOfSelectedItem != selected {
            button.selectItem(withTitle: selected)
        }
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(pick: pick)
    }

    @MainActor
    final class Coordinator: NSObject {
        var pick: (String) -> Void

        init(pick: @escaping (String) -> Void) {
            self.pick = pick
        }

        @objc func changed(_ sender: NSPopUpButton) {
            if let title = sender.titleOfSelectedItem {
                pick(title)
            }
        }
    }
}
