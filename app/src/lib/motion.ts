// Shared motion presets. Springs are tuned close to critical damping so things
// arrive fast (~250 ms) without wobbling.
export const spring = { type: "spring", stiffness: 720, damping: 52, mass: 0.7 } as const;
export const softSpring = { type: "spring", stiffness: 380, damping: 34, mass: 0.9 } as const;
export const easeOut = [0.22, 1, 0.36, 1] as const;
export const fade = { duration: 0.14, ease: easeOut } as const;
