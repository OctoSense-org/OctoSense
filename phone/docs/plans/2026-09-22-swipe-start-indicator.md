# Swipe start indicator

Help users start OctoSense's bottom swipe above Android's system navigation.

- A persistent 12-point upward chevron in a quiet, theme-aware 28 × 20 point capsule marks the existing shell gesture band. Its center sits 14 points above the safe area's bottom, outside Android's navigation inset.
- Reserve another 12 points below the dock and first-use hint so the cue clears both. Keep app labels and page dots unobstructed.
- Name the chevron in the Recents hint. Keep existing gesture recognition and hint persistence. Hide the cue while the keyboard owns the bottom gesture area.
- Compact the Recents footer to keep its app labels above the cue and below split-selection instructions; omit its optional heading when it would collide with a hosted card or split instruction.
- Use the existing vector icon system, with no animation or additional touch handler. The current Home tap target remains in place.

Validation: all 69 mobile regression tests passed and the Android release built successfully. On the Pixel 7 Pro, inspected the home, app-library, hosted-app and Recents footers, and verified scripted swipes and swipe-and-hold from the cue. The final smaller indicator was visually checked on the Pixel; the same APK was installed and launched on the OnePlus 6T. Split-selection clearance was checked in code review. These checks verify scripted paths rather than natural finger usability.
