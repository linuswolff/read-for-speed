import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";

const DEFAULT_WPM = 300;
const MIN_WPM = 100;
const MAX_WPM = 1000;
const WPM_STEP = 20;
const ANCHOR_PERCENT = 43;
const GUIDE_OFFSET_REM = 9;
const GUIDE_TEXT_GAP_REM = 5;
const DEFAULT_FONT_SIZE_REM = 4.5;
const MIN_FONT_SIZE_REM = 2.5;
const MAX_FONT_SIZE_REM = 7;
const FONT_SIZE_STEP_REM = 0.25;
const FONT_SIZE_STORAGE_KEY = "speed.fontSizeRem";
const CLIPBOARD_HISTORY_STORAGE_KEY = "speed.clipboardHistory";
const MAX_CLIPBOARD_HISTORY_ITEMS = 30;

type RenderedTextProps = {
  text: string;
  focalIndex: number;
  className?: string;
  fontSizeRem: number;
};

type ClipboardEntry = {
  id: string;
  text: string;
  preview: string;
  wordCount: number;
  createdAt: number;
};

function clamp(value: number, min: number, max: number) {
  return Math.min(Math.max(value, min), max);
}

function wordsFromInput(value: string) {
  return value.trim().split(/\s+/).filter(Boolean);
}

function focalIndex(length: number) {
  if (length <= 1) {
    return 0;
  }

  return clamp(Math.round((length - 1) * 0.35), 0, length - 1);
}

function makeClipboardPreview(text: string, maxWords = 8) {
  const words = wordsFromInput(text);
  const preview = words.slice(0, maxWords).join(" ");
  return words.length > maxWords ? `${preview}…` : preview;
}

function parseClipboardHistory() {
  if (typeof window === "undefined") {
    return [] as ClipboardEntry[];
  }

  const rawValue = window.localStorage.getItem(CLIPBOARD_HISTORY_STORAGE_KEY);
  if (!rawValue) {
    return [] as ClipboardEntry[];
  }

  try {
    const parsedValue = JSON.parse(rawValue);
    if (!Array.isArray(parsedValue)) {
      return [] as ClipboardEntry[];
    }

    return parsedValue
      .filter(
        (entry): entry is ClipboardEntry =>
          typeof entry?.id === "string" &&
          typeof entry?.text === "string" &&
          typeof entry?.preview === "string" &&
          typeof entry?.wordCount === "number" &&
          typeof entry?.createdAt === "number",
      )
      .slice(0, MAX_CLIPBOARD_HISTORY_ITEMS);
  } catch {
    return [] as ClipboardEntry[];
  }
}

function RenderedText({
  text,
  focalIndex,
  className = "",
  fontSizeRem,
}: RenderedTextProps) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const focalRef = useRef<HTMLSpanElement | null>(null);
  const [offset, setOffset] = useState(0);
  const chars = [...text];
  const safeIndex = clamp(focalIndex, 0, Math.max(chars.length - 1, 0));

  useLayoutEffect(() => {
    const updateOffset = () => {
      const container = containerRef.current;
      const focal = focalRef.current;

      if (!container || !focal) {
        return;
      }

      const focalCenter = focal.offsetLeft + focal.offsetWidth / 2;
      setOffset(focalCenter);
    };

    updateOffset();
    window.addEventListener("resize", updateOffset);

    return () => window.removeEventListener("resize", updateOffset);
  }, [text, safeIndex, fontSizeRem]);

  return (
    <div
      ref={containerRef}
      className={`absolute top-1/2 -translate-y-1/2 whitespace-pre leading-none tracking-tight ${className}`.trim()}
      style={{
        left: `${ANCHOR_PERCENT}%`,
        transform: `translate(${-offset}px, -50%)`,
        fontSize: `${fontSizeRem}rem`,
        fontWeight: 400,
      }}
    >
      {chars.map((char, index) => (
        <span
          key={`${char}-${index}`}
          ref={index === safeIndex ? focalRef : undefined}
          className={index === safeIndex ? "text-[#FF4D4F]" : undefined}
        >
          {char}
        </span>
      ))}
    </div>
  );
}

export default function App() {
  const rootRef = useRef<HTMLDivElement | null>(null);
  const [input, setInput] = useState("");
  const [index, setIndex] = useState(0);
  const [started, setStarted] = useState(false);
  const [playing, setPlaying] = useState(false);
  const [ended, setEnded] = useState(false);
  const [showMenu, setShowMenu] = useState(true);
  const [wpm, setWpm] = useState(DEFAULT_WPM);
  const [fontSizeRem, setFontSizeRem] = useState(() => {
    if (typeof window === "undefined") {
      return DEFAULT_FONT_SIZE_REM;
    }

    const storedValue = window.localStorage.getItem(FONT_SIZE_STORAGE_KEY);
    if (!storedValue) {
      return DEFAULT_FONT_SIZE_REM;
    }

    const parsedValue = Number(storedValue);
    if (Number.isNaN(parsedValue)) {
      return DEFAULT_FONT_SIZE_REM;
    }

    return clamp(parsedValue, MIN_FONT_SIZE_REM, MAX_FONT_SIZE_REM);
  });
  const [clipboardHistory, setClipboardHistory] = useState<ClipboardEntry[]>(() =>
    parseClipboardHistory(),
  );
  const [showClipboardHistory, setShowClipboardHistory] = useState(false);
  const [selectedClipboardIndex, setSelectedClipboardIndex] = useState(0);

  const words = useMemo(() => wordsFromInput(input), [input]);
  const currentWord = words[index] ?? "";

  useEffect(() => {
    rootRef.current?.focus();
  }, []);

  useEffect(() => {
    if (!playing) {
      return;
    }

    const timeout = window.setTimeout(() => {
      setIndex((currentIndex) => {
        if (currentIndex < words.length - 1) {
          return currentIndex + 1;
        }

        setPlaying(false);
        setEnded(true);
        return currentIndex;
      });
    }, 60000 / wpm);

    return () => window.clearTimeout(timeout);
  }, [playing, words.length, wpm, index]);

  useEffect(() => {
    window.localStorage.setItem(FONT_SIZE_STORAGE_KEY, String(fontSizeRem));
  }, [fontSizeRem]);

  useEffect(() => {
    window.localStorage.setItem(
      CLIPBOARD_HISTORY_STORAGE_KEY,
      JSON.stringify(clipboardHistory),
    );
  }, [clipboardHistory]);

  useEffect(() => {
    if (selectedClipboardIndex > clipboardHistory.length - 1) {
      setSelectedClipboardIndex(Math.max(clipboardHistory.length - 1, 0));
    }
  }, [clipboardHistory.length, selectedClipboardIndex]);

  const resetText = (nextInput: string) => {
    setInput(nextInput);
    setIndex(0);
    setStarted(false);
    setPlaying(false);
    setEnded(false);
  };

  const appendInput = (value: string) => {
    if (!value) {
      return;
    }

    resetText(input + value);
  };

  const addClipboardEntry = (value: string) => {
    const normalizedValue = value.trim();
    const wordCount = wordsFromInput(normalizedValue).length;

    if (!normalizedValue || wordCount === 0) {
      return;
    }

    const entry: ClipboardEntry = {
      id:
        typeof crypto !== "undefined" && "randomUUID" in crypto
          ? crypto.randomUUID()
          : `${Date.now()}-${Math.random().toString(16).slice(2)}`,
      text: normalizedValue,
      preview: makeClipboardPreview(normalizedValue),
      wordCount,
      createdAt: Date.now(),
    };

    setClipboardHistory((current) => {
      const deduped = current.filter((item) => item.text !== normalizedValue);
      return [entry, ...deduped].slice(0, MAX_CLIPBOARD_HISTORY_ITEMS);
    });
  };

  const loadClipboardEntry = (entry: ClipboardEntry) => {
    resetText(entry.text);
    setShowClipboardHistory(false);
    setSelectedClipboardIndex(0);
    setTimeout(() => rootRef.current?.focus(), 0);
  };

  const handleKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Tab") {
      return;
    }

    if (showClipboardHistory) {
      switch (event.key) {
        case "Escape":
        case "c":
        case "C":
          event.preventDefault();
          setShowClipboardHistory(false);
          return;
        case "ArrowUp":
          event.preventDefault();
          setSelectedClipboardIndex((current) => Math.max(current - 1, 0));
          return;
        case "ArrowDown":
          event.preventDefault();
          setSelectedClipboardIndex((current) =>
            Math.min(current + 1, Math.max(clipboardHistory.length - 1, 0)),
          );
          return;
        case "Enter": {
          event.preventDefault();
          const selectedEntry = clipboardHistory[selectedClipboardIndex];
          if (selectedEntry) {
            loadClipboardEntry(selectedEntry);
          }
          return;
        }
        default:
          event.preventDefault();
          return;
      }
    }

    if (event.ctrlKey || event.metaKey) {
      if (event.key.toLowerCase() === "h") {
        event.preventDefault();
        setShowMenu((current) => !current);
      }
      return;
    }

    switch (event.key) {
      case "c":
      case "C":
        event.preventDefault();
        if (!playing && clipboardHistory.length > 0) {
          setSelectedClipboardIndex(0);
          setShowClipboardHistory(true);
        }
        return;
      case "h":
      case "H":
        event.preventDefault();
        setShowMenu((current) => !current);
        return;
      case "ArrowUp":
        event.preventDefault();
        setWpm((current) => clamp(current + WPM_STEP, MIN_WPM, MAX_WPM));
        return;
      case "ArrowDown":
        event.preventDefault();
        setWpm((current) => clamp(current - WPM_STEP, MIN_WPM, MAX_WPM));
        return;
      case "+":
      case "=":
        event.preventDefault();
        setFontSizeRem((current) =>
          clamp(
            current + FONT_SIZE_STEP_REM,
            MIN_FONT_SIZE_REM,
            MAX_FONT_SIZE_REM,
          ),
        );
        return;
      case "-":
      case "_":
        event.preventDefault();
        setFontSizeRem((current) =>
          clamp(
            current - FONT_SIZE_STEP_REM,
            MIN_FONT_SIZE_REM,
            MAX_FONT_SIZE_REM,
          ),
        );
        return;
      case "0":
        event.preventDefault();
        setFontSizeRem(DEFAULT_FONT_SIZE_REM);
        return;
      case " ":
        event.preventDefault();
        if (words.length === 0 || ended) {
          return;
        }
        if (!started) {
          setStarted(true);
          setPlaying(true);
          return;
        }
        setPlaying((current) => !current);
        return;
      case "Enter":
        event.preventDefault();
        if (!playing) {
          resetText(input + " ");
        }
        return;
      case "Backspace":
        event.preventDefault();
        if (!playing && input.length > 0) {
          resetText(input.slice(0, -1));
        }
        return;
      default:
        break;
    }

    if (!playing && event.key.length === 1 && !event.altKey) {
      event.preventDefault();
      appendInput(event.key);
    }
  };

  const handlePaste = (event: React.ClipboardEvent<HTMLDivElement>) => {
    if (playing) {
      return;
    }

    const pastedText = event.clipboardData.getData("text");
    if (!pastedText) {
      return;
    }

    event.preventDefault();
    addClipboardEntry(pastedText);
    appendInput(pastedText);
  };

  const prompt = words.length === 0 ? "Paste some text" : "▶";
  const promptFocalIndex =
    words.length === 0
      ? [..."Paste some text"].findIndex((char) => char === "o")
      : 0;

  return (
    <div
      className="relative h-screen w-screen bg-black text-[#D4D7DE] flex items-center justify-center"
    >
      {/* lw backlink — pinned top-left of viewport */}
      <div className="absolute top-6 left-6">
        <a
          href="https://linuswolff.github.io/"
          className="text-2xl font-bold tracking-tighter text-white hover:opacity-70 transition-opacity"
        >
          lw
        </a>
      </div>

      {/* Reader box */}
      <div
        ref={rootRef}
        tabIndex={0}
        onKeyDown={handleKeyDown}
        onPaste={handlePaste}
        onMouseDown={() => rootRef.current?.focus()}
        className="relative overflow-hidden bg-[#090A0D] border-2 border-[#1E2128] outline-none rounded-xl"
        style={{ width: 'min(900px, min(90vw, 135vh))', aspectRatio: '3/2' }}
      >
      <div
        className="absolute inset-x-0 border-t border-[#565B65]"
        style={{ top: `calc(50% - ${GUIDE_OFFSET_REM}rem)` }}
      >
        <div
          className="absolute h-3 w-px -translate-x-1/2 bg-[#565B65]"
          style={{ left: `${ANCHOR_PERCENT}%`, top: 0 }}
        />
      </div>

      <div
        className="absolute inset-x-0 border-t border-[#565B65]"
        style={{ top: `calc(50% + ${GUIDE_OFFSET_REM}rem)` }}
      >
        <div
          className="absolute bottom-0 h-3 w-px -translate-x-1/2 bg-[#565B65]"
          style={{ left: `${ANCHOR_PERCENT}%` }}
        />
      </div>

      <div
        className="absolute w-px bg-[#565B65]"
        style={{
          left: `${ANCHOR_PERCENT}%`,
          top: `calc(50% - ${GUIDE_OFFSET_REM}rem)`,
          height: `calc(${GUIDE_OFFSET_REM}rem - ${GUIDE_TEXT_GAP_REM}rem)`,
          transform: "translateX(-50%)",
        }}
      />

      <div
        className="absolute w-px bg-[#565B65]"
        style={{
          left: `${ANCHOR_PERCENT}%`,
          top: `calc(50% + ${GUIDE_TEXT_GAP_REM}rem)`,
          height: `calc(${GUIDE_OFFSET_REM}rem - ${GUIDE_TEXT_GAP_REM}rem)`,
          transform: "translateX(-50%)",
        }}
      />

      {words.length === 0 || !started ? (
        <RenderedText
          text={prompt}
          focalIndex={promptFocalIndex}
          className="text-[#C5CAD3]"
          fontSizeRem={fontSizeRem}
        />
      ) : (
        <RenderedText
          text={currentWord}
          focalIndex={focalIndex([...currentWord].length)}
          fontSizeRem={fontSizeRem}
        />
      )}

      {showMenu && !playing ? (
        <div className="absolute inset-x-0 bottom-0 flex items-center justify-between px-4 pb-3 text-sm leading-none">
          <div className="text-[#7E8591]">
            <span>Space play/pause</span>
            <span className="text-[#5F6672]"> · c history · h hide · +/- size</span>
          </div>
          <div className="text-[#7E8591]">↑↓ {wpm} wpm</div>
        </div>
      ) : null}

      {showClipboardHistory ? (
        <div className="absolute inset-0 z-10 flex items-center justify-center bg-black/45 px-5">
          <div className="w-full max-w-3xl rounded-2xl border border-[#2A2F38] bg-[#111318]/95 p-3 shadow-2xl backdrop-blur-sm">
            <div className="mb-3 flex items-center justify-between px-2">
              <div className="text-sm font-medium text-[#C5CAD3]">Clipboard history</div>
              <div className="text-xs text-[#5F6672]">↑↓ select · enter load · esc close</div>
            </div>

            {clipboardHistory.length === 0 ? (
              <div className="px-2 py-10 text-sm text-[#7E8591]">No clipboard history yet.</div>
            ) : (
              <div className="max-h-[60vh] space-y-2 overflow-y-auto pr-1">
                {clipboardHistory.map((entry, historyIndex) => {
                  const isSelected = historyIndex === selectedClipboardIndex;

                  return (
                    <button
                      key={entry.id}
                      type="button"
                      onClick={() => loadClipboardEntry(entry)}
                      onMouseEnter={() => setSelectedClipboardIndex(historyIndex)}
                      className={`block w-full rounded-xl border px-4 py-3 text-left transition-colors ${
                        isSelected
                          ? "border-[#565B65] bg-[#1A1D23] text-[#D4D7DE]"
                          : "border-[#20242B] bg-[#0D0F13] text-[#A8AFBB] hover:bg-[#14171C]"
                      }`}
                    >
                      <div className="truncate text-base leading-snug">{entry.preview}</div>
                      <div className="mt-1 text-xs text-[#6D7480]">{entry.wordCount} words</div>
                    </button>
                  );
                })}
              </div>
            )}
          </div>
        </div>
      ) : null}
      </div>

      {/* Desktop app tease — pinned, centered between box bottom and viewport bottom */}
      <a
        href="https://github.com/linuswolff/read-for-speed"
        target="_blank"
        rel="noopener noreferrer"
        className="absolute flex flex-col items-center gap-3 hover:opacity-70 transition-opacity duration-300"
        style={{ top: 'calc(50% + min(450px, min(45vw, 67.5vh)) + (50vh - min(450px, min(45vw, 67.5vh))) / 2)', transform: 'translateY(-50%)' }}
      >
        {/* App icon: rounded rect with vertical line + red dot */}
        <svg width="40" height="40" viewBox="0 0 40 40" fill="none" xmlns="http://www.w3.org/2000/svg">
          <rect width="40" height="40" rx="9" fill="#141618"/>
          <line x1="20" y1="6" x2="20" y2="34" stroke="#565B65" strokeWidth="1.5" strokeLinecap="round"/>
          <circle cx="20" cy="20" r="6" fill="#FF4D4F"/>
        </svg>
        <span className="text-xs text-[#565B65] tracking-wide">Also available as a desktop app</span>
      </a>
    </div>
  );
}
