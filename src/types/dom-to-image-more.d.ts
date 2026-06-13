declare module 'dom-to-image-more' {
  interface Options {
    quality?: number;
    width?: number | null;
    height?: number | null;
    style?: Record<string, string>;
    filter?: (node: Node) => boolean;
    imagePlaceholder?: string;
    cacheBust?: boolean;
  }

  function toBlob(node: Node, options?: Options): Promise<Blob>;
  function toPng(node: Node, options?: Options): Promise<string>;
  function toJpeg(node: Node, options?: Options): Promise<string>;
  function toSvg(node: Node, options?: Options): Promise<string>;
  function toJwt(node: Node, options?: Options): Promise<string>;
  function toPixelData(node: Node, options?: Options): Promise<Uint8Array>;

  export { toBlob, toPng, toJpeg, toSvg, toJwt, toPixelData };
}