import type { ReactNode } from 'react';

interface IconProps {
  name:
    | 'airfoil'
    | 'mesh'
    | 'flow'
    | 'setup'
    | 'upload'
    | 'download'
    | 'reset'
    | 'play'
    | 'check'
    | 'warning'
    | 'error'
    | 'zoom-in'
    | 'zoom-out'
    | 'fit'
    | 'file'
    | 'code';
  size?: number;
}

export function Icon({ name, size = 18 }: IconProps): ReactNode {
  const paths: Record<IconProps['name'], ReactNode> = {
    airfoil: <path d="M3 13.2c4.8-4.6 10.8-5.7 18-2.3-7.2 1.5-13.2 2.3-18 2.3Z" />,
    mesh: (
      <>
        <path d="M4 4h16v16H4zM4 10h16M4 16h16M10 4v16M16 4v16" />
      </>
    ),
    flow: (
      <>
        <path d="M3 7h12c2 0 3-1 3-2.5S17 2 15.5 2c-1.2 0-2 .7-2.3 1.5" />
        <path d="M3 12h16c1.2 0 2 .8 2 2s-.8 2-2 2h-1" />
        <path d="M3 17h9c1.5 0 2.5 1 2.5 2.2S13.6 21 12.5 21c-.9 0-1.5-.4-1.8-1" />
      </>
    ),
    setup: (
      <>
        <circle cx="12" cy="12" r="3" />
        <path d="M19.4 15a1.8 1.8 0 0 0 .4 2l.1.1-2.8 2.8-.1-.1a1.8 1.8 0 0 0-2-.4 1.8 1.8 0 0 0-1.1 1.7V21h-4v-.1A1.8 1.8 0 0 0 8.8 19a1.8 1.8 0 0 0-2 .4l-.1.1-2.8-2.8.1-.1a1.8 1.8 0 0 0 .4-2A1.8 1.8 0 0 0 2.7 13H2v-4h.7a1.8 1.8 0 0 0 1.7-1.1 1.8 1.8 0 0 0-.4-2l-.1-.1L6.7 3l.1.1a1.8 1.8 0 0 0 2 .4A1.8 1.8 0 0 0 10 1.8V2h4v-.2a1.8 1.8 0 0 0 1.1 1.7 1.8 1.8 0 0 0 2-.4l.1-.1L20 5.8l-.1.1a1.8 1.8 0 0 0-.4 2A1.8 1.8 0 0 0 21.2 9h.8v4h-.8a1.8 1.8 0 0 0-1.8 2Z" />
      </>
    ),
    upload: <path d="M12 16V3m0 0L7 8m5-5 5 5M4 14v6h16v-6" />,
    download: <path d="M12 3v13m0 0 5-5m-5 5-5-5M4 20h16" />,
    reset: (
      <>
        <path d="M4 8V3m0 0h5M4 3l3.2 3.2a8 8 0 1 1-2 8.1" />
      </>
    ),
    play: <path d="m8 5 11 7-11 7V5Z" />,
    check: <path d="m4 12 5 5L20 6" />,
    warning: <path d="M12 3 2.8 20h18.4L12 3Zm0 6v5m0 3v.1" />,
    error: (
      <>
        <circle cx="12" cy="12" r="9" />
        <path d="m9 9 6 6m0-6-6 6" />
      </>
    ),
    'zoom-in': <path d="M11 7v8m-4-4h8m4 8-4-4m2-4a6 6 0 1 1-12 0 6 6 0 0 1 12 0Z" />,
    'zoom-out': <path d="M7 11h8m4 8-4-4m2-4a6 6 0 1 1-12 0 6 6 0 0 1 12 0Z" />,
    fit: <path d="M9 4H4v5m11-5h5v5M9 20H4v-5m11 5h5v-5" />,
    file: <path d="M6 2h8l4 4v16H6V2Zm8 0v5h5M9 12h6m-6 4h6" />,
    code: <path d="m8 8-4 4 4 4m8-8 4 4-4 4m-2-10-4 12" />,
  };

  return (
    <svg
      aria-hidden="true"
      className="icon"
      fill="none"
      height={size}
      viewBox="0 0 24 24"
      width={size}
    >
      <g
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="1.7"
      >
        {paths[name]}
      </g>
    </svg>
  );
}
