"""Inspect the owned Xvfb fixture's actual XEmbed hierarchy; no screen capture."""
import ctypes as c
import os


def inspect_embedding(*, allow_empty=False):
    """Return live child geometry, or None when explicitly allowing a closed view."""
    lib = c.CDLL('libX11.so.6')
    display_t = c.c_void_p
    window_t = c.c_ulong
    lib.XOpenDisplay.argtypes = [c.c_char_p]
    lib.XOpenDisplay.restype = display_t
    lib.XDefaultRootWindow.argtypes = [display_t]
    lib.XDefaultRootWindow.restype = window_t
    lib.XQueryTree.argtypes = [display_t, window_t, c.POINTER(window_t), c.POINTER(window_t), c.POINTER(c.POINTER(window_t)), c.POINTER(c.c_uint)]
    lib.XInternAtom.argtypes = [display_t, c.c_char_p, c.c_int]
    lib.XInternAtom.restype = c.c_ulong
    lib.XGetWindowProperty.argtypes = [display_t, window_t, c.c_ulong, c.c_long, c.c_long, c.c_int, c.c_ulong, c.POINTER(c.c_ulong), c.POINTER(c.c_int), c.POINTER(c.c_ulong), c.POINTER(c.c_ulong), c.POINTER(c.c_void_p)]
    lib.XGetGeometry.argtypes = [display_t, window_t, c.POINTER(window_t), c.POINTER(c.c_int), c.POINTER(c.c_int), c.POINTER(c.c_uint), c.POINTER(c.c_uint), c.POINTER(c.c_uint), c.POINTER(c.c_uint)]
    lib.XFree.argtypes = [c.c_void_p]
    lib.XCloseDisplay.argtypes = [display_t]
    display = lib.XOpenDisplay(os.environ['DISPLAY'].encode())
    assert display, 'Owned X display is unavailable'
    try:
        root = lib.XDefaultRootWindow(display)
        atom = lib.XInternAtom(display, b'_XEMBED_INFO', 0)
        parents, plugs = {}, []
        queue = [root]
        while queue:
            window = queue.pop()
            tree_root, parent = window_t(), window_t()
            children, count = c.POINTER(window_t)(), c.c_uint()
            if not lib.XQueryTree(display, window, c.byref(tree_root), c.byref(parent), c.byref(children), c.byref(count)):
                continue
            parents[window] = parent.value
            queue.extend(children[index] for index in range(min(count.value, 256)))
            if children: lib.XFree(children)
            assert len(parents) < 256, 'Unexpectedly large owned X fixture'
            actual, fmt, items, after, data = c.c_ulong(), c.c_int(), c.c_ulong(), c.c_ulong(), c.c_void_p()
            status = lib.XGetWindowProperty(display, window, atom, 0, 2, 0, 0, c.byref(actual), c.byref(fmt), c.byref(items), c.byref(after), c.byref(data))
            if status == 0 and fmt.value == 32 and items.value >= 2:
                plugs.append(window)
            if data: lib.XFree(data)
        if allow_empty and not plugs:
            return None
        assert len(plugs) == 1, f'Expected one embedded GtkPlug, got {len(plugs)}'
        plug = plugs[0]
        socket = parents[plug]
        host = parents[socket]
        assert host != root and parents[host] == root, 'GTK browser is not a child of the existing app window'
        def geometry(window):
            r, x, y, width, height, border, depth = window_t(), c.c_int(), c.c_int(), c.c_uint(), c.c_uint(), c.c_uint(), c.c_uint()
            assert lib.XGetGeometry(display, window, c.byref(r), c.byref(x), c.byref(y), c.byref(width), c.byref(height), c.byref(border), c.byref(depth))
            return dict(x=x.value, y=y.value, width=width.value, height=height.value)
        result = {'hierarchy': 'GtkPlug -> native child socket -> Makepad window -> X root', 'plug': geometry(plug), 'socket': geometry(socket), 'host': geometry(host)}
        assert result['socket']['width'] > 100 and result['socket']['height'] > 100
        assert result['plug']['width'] >= result['socket']['width'] and result['plug']['height'] >= result['socket']['height']
        return result
    finally:
        lib.XCloseDisplay(display)
