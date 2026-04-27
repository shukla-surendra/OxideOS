  Workflow: Draw icons in any pixel editor (Aseprite, GIMP, Krita) → export as 16×16 or 24×24 PNG → convert to raw bytes
  with ffmpeg -i icon.png -f rawvideo -pix_fmt gray icon.raw → include_bytes!() in Rust. SVG is not viable — it needs a
  renderer (resvg, librsvg) that depends on std/libc. Linux uses XDG icon themes (PNG/SVG via librsvg) only in userspace;
   the kernel itself uses raw bitmaps.
