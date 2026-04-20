import { useEffect, useRef } from "react";

interface Props {
    children: React.ReactNode;
    className?: string;
}

// Keyboard-inset fallback.
//
// Modern browsers (iOS Safari 17.4+, Chrome Android) respect
// `interactive-widget=resizes-content` in the viewport meta and shrink
// the layout viewport when the software keyboard opens, which
// automatically lifts our bottom bar above it. Older browsers overlay
// the keyboard on top of fixed-bottom content instead.
//
// This effect polyfills that behaviour: it listens to VisualViewport
// resizes, computes `innerHeight - visualViewport.height` as the
// keyboard inset, and writes it to `padding-bottom` alongside the
// normal safe-area inset. On supported browsers the value stays at 0
// because the layout viewport has already done the work.
function useKeyboardInset(ref: React.RefObject<HTMLElement | null>) {
    useEffect(() => {
        const vv = window.visualViewport;
        if (!vv) return;
        const el = ref.current;
        if (!el) return;
        const update = () => {
            const inset = Math.max(
                0,
                Math.round(window.innerHeight - vv.height - vv.offsetTop),
            );
            el.style.setProperty("--keyboard-inset", `${inset}px`);
        };
        update();
        vv.addEventListener("resize", update);
        vv.addEventListener("scroll", update);
        return () => {
            vv.removeEventListener("resize", update);
            vv.removeEventListener("scroll", update);
        };
    }, [ref]);
}

export default function Screen({ children, className = "" }: Props) {
    const rootRef = useRef<HTMLDivElement | null>(null);
    useKeyboardInset(rootRef);
    // No safe-area-inset-bottom reserve here — routes that have a
    // bottom-anchored chrome (the workspace KeyBar) extend their own
    // background into the safe area and apply the inset as inner
    // padding so the home indicator sits on glass instead of bare
    // app-base. Routes without bottom chrome add their own padding.
    return (
        <div
            ref={rootRef}
            className={`h-full w-full flex flex-col bg-app-base text-app-text ${className}`}
            style={{
                paddingTop: "env(safe-area-inset-top)",
                paddingBottom: "var(--keyboard-inset, 0px)",
            }}
        >
            {children}
        </div>
    );
}
