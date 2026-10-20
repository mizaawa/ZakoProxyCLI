// Zako-chan: a small sakura fish ("zako" means small fry). Purely decorative;
// colours are inline so the drawing stays intact even before styles load.
export function Mascot({ size = 56 }: { size?: number }) {
  return (
    <svg
      className="mascot"
      width={size}
      height={size}
      viewBox="0 0 64 64"
      aria-hidden="true"
      focusable="false"
    >
      <path
        d="M47 32 L60 21 Q63 32 60 43 Z"
        fill="#ffd66b"
        stroke="#e2a72a"
        strokeWidth="2"
        strokeLinejoin="round"
      />
      <path
        d="M21 20 Q29 6 42 20 Z"
        fill="#ffd66b"
        stroke="#e2a72a"
        strokeWidth="2"
        strokeLinejoin="round"
      />
      <ellipse cx="29" cy="33" rx="21" ry="17" fill="#ff8ab6" stroke="#d6377a" strokeWidth="2" />
      <ellipse cx="34" cy="22" rx="6" ry="3" fill="#fff" opacity="0.6" transform="rotate(-15 34 22)" />
      <path
        d="M32 39 Q37 47 43 41 Q37 42.5 32 39 Z"
        fill="#ffd66b"
        stroke="#e2a72a"
        strokeWidth="1.6"
        strokeLinejoin="round"
      />
      <circle cx="20" cy="30" r="5.5" fill="#4a2a3d" />
      <circle cx="18.2" cy="28.2" r="2.2" fill="#fff" />
      <circle cx="21.8" cy="32.4" r="1" fill="#fff" />
      <ellipse cx="22.5" cy="39.5" rx="3.4" ry="1.8" fill="#ff5c93" opacity="0.5" />
      <path
        d="M11 36.5 Q12.5 39 14 36.5 Q15.5 39 17 36.5"
        fill="none"
        stroke="#4a2a3d"
        strokeWidth="1.6"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path d="M52 6 L53.4 10.6 L58 12 L53.4 13.4 L52 18 L50.6 13.4 L46 12 L50.6 10.6 Z" fill="#ffc53d" />
      <circle cx="8" cy="15" r="2" fill="#ffb3cf" />
    </svg>
  );
}
