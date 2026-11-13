// Shared motion presets. Springs are tuned close to critical damping so things
// arrive fast (~250 ms) without wobbling.
export const spring = { type: "spring", stiffness: 520, damping: 42, mass: 0.8 } as const;
export const softSpring = { type: "spring", stiffness: 380, damping: 34, mass: 0.9 } as const;
export const easeOut = [0.22, 1, 0.36, 1] as const;
export const fade = { duration: 0.18, ease: easeOut } as const;
