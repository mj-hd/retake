export default function BrandMark() {
  return (
    <svg className="brand-mark" viewBox="0 0 128 128" aria-hidden="true">
      <g className="brand-mark-frame">
        <path d="M49 16H31A15 15 0 0 0 16 31V49" />
        <path d="M79 16H97A15 15 0 0 1 112 31V49" />
        <path d="M49 112H31A15 15 0 0 1 16 97V79" />
        <path d="M79 112H97A15 15 0 0 0 112 97V79" />
      </g>
      <circle className="brand-mark-lens" cx="64" cy="64" r="26" />
    </svg>
  );
}
