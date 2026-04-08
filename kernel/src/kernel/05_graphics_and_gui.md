# 5. Graphics & GUI

Modern operating systems require a graphical user interface (GUI). OxideOS implements a basic windowing system built on a layered architecture. This document explains the components of the graphics stack, from writing raw pixels to managing interactive windows.

---

### Layer 1: The Linear Framebuffer

The foundation of the entire graphics system is the **linear framebuffer**. Instead of dealing with complex, legacy VGA text modes, OxideOS requests a simple, contiguous block of memory from the Limine bootloader (`FramebufferRequest`).

*   **Concept**: This memory region maps directly to the screen. Each group of 4 bytes in this memory represents the color of a single pixel. To draw something, the kernel simply writes color values to the correct memory addresses.
*   **Pixel Format**: OxideOS typically uses a 32-bit ARGB (Alpha, Red, Green, Blue) or XRGB format, where each color component gets 8 bits.

### Layer 2: The `Graphics` Struct

Writing directly to the raw framebuffer pointer is unsafe and inconvenient. The `Graphics` struct provides a safe Rust abstraction over it.

*   **Role**: It wraps the raw pointer and screen dimensions (width, height) and provides a set of primitive drawing operations. This encapsulates the `unsafe` code in one place.
*   **Primitives**:
    *   `draw_pixel(x, y, color)`: The most basic operation.
    *   `fill_rect(x, y, width, height, color)`: A more efficient way to draw solid rectangles.
    *   `draw_string(x, y, text, color)`: Renders text using a built-in bitmap font.
*   **Double Buffering**: To avoid flickering and visual artifacts (a phenomenon called "tearing"), the `Graphics` struct implements double buffering. It doesn't draw directly to the visible framebuffer. Instead, it draws to an off-screen memory buffer (the "back buffer"). When all drawing for a frame is complete, the `present()` method copies the entire back buffer to the visible framebuffer in one fast operation.

### Layer 3: The `WindowManager`

The `WindowManager` is responsible for orchestrating what appears on the screen. It manages a collection of `Window` objects.

*   **Window Management**: It keeps track of all windows, their positions, sizes, and Z-order (which windows are on top of others).
*   **Drawing Algorithm**: The window manager uses a classic **Painter's Algorithm** for rendering the scene. It draws objects from back to front:
    1.  Draw the desktop background.
    2.  Draw the taskbar.
    3.  Iterate through the list of windows from the bottom-most to the top-most, drawing each one.
*   **Focus and Interaction**: It determines which window is currently "focused" (i.e., should receive keyboard input) and handles mouse events like clicks (to change focus) and drags (to move windows).

### The Main GUI Event Loop

The final stage of the kernel's `kmain` function is to enter the main GUI loop (`run_gui_with_mouse`). This is an infinite loop that forms a cooperative, event-driven system.

A single cycle of the loop performs these actions:

1.  **Poll Input**: It checks for new mouse data from the interrupt system (`interrupts::poll_mouse_data()`).
2.  **Process Events**: It compares the current mouse position and button state with the state from the previous loop cycle. This allows it to detect discrete events like `mouse_down`, `mouse_up`, and `mouse_drag`.
3.  **Update State**: Based on the detected events, it calls methods on the `WindowManager` (e.g., `handle_click`, `handle_drag`). If any window's state changes, a `needs_redraw` flag is set to `true`. The clock update also forces a redraw once per second.
4.  **Redraw Scene**: If `needs_redraw` is true, the entire scene is redrawn using the Painter's Algorithm described above. This is done into the back buffer.
5.  **Draw Cursor**: After the scene is drawn, the cursor is drawn on top. To do this without corrupting the underlying image, it uses a "save-under" technique: it first saves the small rectangle of pixels where the cursor will be, draws the cursor, and then restores the saved pixels on the next frame before the cursor moves.
6.  **Present**: The `graphics.present()` method is called to blit the completed back buffer to the screen.
7.  **Sleep (`hlt`)**: This is one of the most important steps for efficiency. The `hlt` (Halt) instruction tells the CPU to stop executing instructions and enter a low-power sleep state. The CPU will remain halted until the next hardware interrupt occurs (e.g., a timer tick or a mouse movement). When the interrupt handler finishes, execution resumes right after the `hlt`. Without this, the event loop would spin at full speed, consuming 100% of the CPU and wasting power.