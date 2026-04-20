// Full-screen QR scanner. Opens the rear camera, runs a jsQR decode
// loop on video frames, and fires `onDetected` the first time the
// decoded payload looks like a host id.
//
// Why jsQR: the host QR has a small SuperHQ logo in the middle.
// qr-scanner's WASM decoder (and the BarcodeDetector path it prefers
// on iOS) both stumble on QRs with visible center occlusion, even at
// EcLevel::H. jsQR's Reed-Solomon recovery handles it gracefully.

import { useEffect, useRef, useState } from "react";
import jsQR from "jsqr";

interface Props {
    onDetected: (payload: string) => void;
    onClose: () => void;
}

/// Accept only iroh-node-id-shaped payloads: 64 lowercase hex chars.
/// Random scans (logos, adjacent text, partial reads) are ignored and
/// the decode loop keeps running.
const HOST_ID_RE = /^[0-9a-f]{64}$/i;

export default function QrScannerModal({ onDetected, onClose }: Props) {
    const videoRef = useRef<HTMLVideoElement | null>(null);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        const video = videoRef.current;
        if (!video) return;
        let stream: MediaStream | null = null;
        let raf = 0;
        let stopped = false;
        const canvas = document.createElement("canvas");
        const ctx = canvas.getContext("2d", { willReadFrequently: true });

        (async () => {
            try {
                stream = await navigator.mediaDevices.getUserMedia({
                    video: { facingMode: { ideal: "environment" } },
                    audio: false,
                });
                if (stopped) {
                    stream.getTracks().forEach((t) => t.stop());
                    return;
                }
                video.srcObject = stream;
                await video.play();
            } catch (e) {
                setError(
                    e instanceof Error ? e.message : "Couldn't start the camera.",
                );
                return;
            }

            const tick = () => {
                if (stopped || !ctx) return;
                if (video.readyState >= video.HAVE_ENOUGH_DATA) {
                    const w = video.videoWidth;
                    const h = video.videoHeight;
                    if (w && h) {
                        canvas.width = w;
                        canvas.height = h;
                        ctx.drawImage(video, 0, 0, w, h);
                        const img = ctx.getImageData(0, 0, w, h);
                        const code = jsQR(img.data, w, h, {
                            inversionAttempts: "attemptBoth",
                        });
                        if (code) {
                            const payload = code.data.trim();
                            if (HOST_ID_RE.test(payload)) {
                                stopped = true;
                                onDetected(payload);
                                return;
                            }
                        }
                    }
                }
                raf = requestAnimationFrame(tick);
            };
            raf = requestAnimationFrame(tick);
        })();

        return () => {
            stopped = true;
            if (raf) cancelAnimationFrame(raf);
            if (stream) stream.getTracks().forEach((t) => t.stop());
            if (video.srcObject) video.srcObject = null;
        };
    }, [onDetected]);

    return (
        <div className="fixed inset-0 z-50 flex flex-col bg-black">
            <div
                className="flex items-center justify-between px-4 py-3"
                style={{ paddingTop: "calc(env(safe-area-inset-top) + 12px)" }}
            >
                <div className="text-sm font-medium text-white">
                    Scan QR code
                </div>
                <button
                    onClick={onClose}
                    className="rounded-md px-3 py-1.5 text-sm text-white/80 active:bg-white/10"
                >
                    Close
                </button>
            </div>
            <div className="relative flex-1 overflow-hidden">
                <video
                    ref={videoRef}
                    className="h-full w-full object-cover"
                    playsInline
                    muted
                />
                <ScanOverlay />
                {error ? (
                    <div className="pointer-events-none absolute inset-x-6 bottom-20 text-center text-sm text-red-300">
                        {error}
                    </div>
                ) : null}
            </div>
            <div
                className="px-6 py-4 text-center text-sm text-white/70"
                style={{
                    paddingBottom: "calc(env(safe-area-inset-bottom) + 16px)",
                }}
            >
                Point your camera at the QR code in the SuperHQ popover.
            </div>
        </div>
    );
}

/// Four corner brackets centered on the viewport, purely decorative.
/// jsQR scans the full video frame regardless, so the framing is a
/// visual cue rather than a hard region.
function ScanOverlay() {
    const corner =
        "absolute h-10 w-10 border-yellow-400 pointer-events-none";
    return (
        <div className="pointer-events-none absolute inset-0 flex items-center justify-center">
            <div className="relative h-[72vw] max-h-[60vh] w-[72vw] max-w-[60vh]">
                <div className={`${corner} top-0 left-0 border-t-[3px] border-l-[3px] rounded-tl-lg`} />
                <div className={`${corner} top-0 right-0 border-t-[3px] border-r-[3px] rounded-tr-lg`} />
                <div className={`${corner} bottom-0 left-0 border-b-[3px] border-l-[3px] rounded-bl-lg`} />
                <div className={`${corner} bottom-0 right-0 border-b-[3px] border-r-[3px] rounded-br-lg`} />
            </div>
        </div>
    );
}
