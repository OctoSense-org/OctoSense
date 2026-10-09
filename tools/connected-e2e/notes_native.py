"""Semantic selectors for the Rinx writer's icon-only Notes controls."""
from native import Native, SELECT_ALL


class NotesNative(Native):
    def icon(self, identifier):
        row = self.find(identifier=identifier)
        width, height = self.call('s')['w'][0]['sz']
        if row:
            x, y, w, h = row['r']
            if x >= 0 and y >= 30 and x+w <= width and y+h <= height and min(w,h) >= 28:
                self.click_row(row)
                return
        self.click_row(self.reachable(identifier=identifier))

    def field(self, identifier, value):
        if identifier != 'markdown':
            return super().field(identifier, value)
        # The original Rinx source area extends to the window edge on desktop.
        # Its first visible line is a valid input target; a padding requirement
        # on the entire Fill container incorrectly rejects this layout.
        row = self.wait(lambda: self.find(identifier=identifier, kind='TextInput'))
        x, y, w, h = row['r']
        width, height = self.call('s')['w'][0]['sz']
        px, py = x + min(24, w/2), y + min(24, h/2)
        assert 0 <= px < width and 30 <= py < height
        self.call('click', x=px, y=py, wait=1)
        self.call('k', k='press', c='KeyA', wait=1, **SELECT_ALL)
        self.call('t', t=value, wait=1)
        self.wait(lambda: (self.find(identifier=identifier, kind='TextInput') or {}).get('t') == value)
        self.actions.append({'edit': identifier, 'characters': len(value)})

    def tab(self, text):
        modes = {'Markdown': 'source_mode', 'Preview': 'preview_mode',
                 'Split': 'split_mode', 'Write': 'rich_mode'}
        identifier = modes[text]
        if text == 'Write':
            self.styles()
            identifier = ('rich_mode' if self.find(identifier='rich_mode')
                          else 'mobile_rich_mode')
        self.icon(identifier)

    def styles(self):
        if self.find(identifier='palette_button'):
            self.icon('palette_button')
        else:
            self.icon('preview_mode')
            self.icon('style_fab')

    def click(self, text):
        controls = {'Repository': 'repository_button', 'Save': 'save_button'}
        if text in controls:
            self.icon(controls[text])
        else:
            super().click(text)
