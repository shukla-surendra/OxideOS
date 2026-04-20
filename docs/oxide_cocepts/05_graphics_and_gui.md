# Chapter 5: Graphics & GUI - Bringing the OS to Life Visually

Modern operating systems require a graphical user interface (GUI). OxideOS implements a basic windowing system built on a layered architecture. This document explains the components of the graphics stack, from writing raw pixels to managing interactive windows.

---

### Layer 1: The Linear Framebuffer

The very foundation of OxideOS's graphical system is the **linear framebuffer**. Instead of wrestling with complex, outdated VGA text modes, OxideOS takes a modern approach: it requests a simple, contiguous block of memory from the Limine bootloader using a `FramebufferRequest` (as seen in Chapter 1).

*   **Concept**: This special memory region is directly mapped to your computer screen. Imagine it as a giant grid where each tiny square (a pixel) on your screen corresponds to a specific set of bytes in this memory block. To draw anything, the kernel simply writes color values to the correct memory addresses within this framebuffer. It's like painting directly onto the screen's memory.
*   **Pixel Format**: OxideOS typically uses a 32-bit color format, often ARGB (Alpha, Red, Green, Blue) or XRGB. In this format, each of the four color components (or three, plus an unused "X" byte) gets 8 bits, allowing for over 16 million different colors per pixel.
*   **Direct Access**: The kernel receives a raw pointer to this memory. This direct access is incredibly fast for drawing, but also requires careful handling to avoid memory corruption.

### Layer 2: The `Graphics` Struct

Writing directly to a raw, `unsafe` framebuffer pointer is prone to errors and inconvenient. To make graphics programming safer and easier, OxideOS introduces the `Graphics` struct, which provides a safe Rust abstraction over the raw framebuffer.

*   **Role**: The `Graphics` struct encapsulates the raw framebuffer pointer, along with screen dimensions (width and height). Its primary role is to provide a set of higher-level, safe drawing operations, effectively hiding the `unsafe` low-level memory access within its methods.
*   **Primitives**: It offers fundamental drawing primitives:
    *   `draw_pixel(x, y, color)`: The most basic operation, setting the color of a single pixel.
    *   `fill_rect(x, y, width, height, color)`: A more efficient way to draw solid, colored rectangles.
    *   `draw_string(x, y, text, color)`: Renders text onto the screen using a built-in bitmap font.
*   **Double Buffering**: To prevent visual artifacts like flickering and "tearing" (where you see parts of an old frame mixed with a new one), the `Graphics` struct implements **double buffering**. This means it doesn't draw directly to the visible framebuffer. Instead, all drawing operations are performed on an off-screen memory buffer, often called the "back buffer." Only when all drawing for a complete frame is finished does the `present()` method copy the entire contents of this back buffer to the visible framebuffer in one swift, atomic operation. This ensures a smooth, flicker-free display.

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