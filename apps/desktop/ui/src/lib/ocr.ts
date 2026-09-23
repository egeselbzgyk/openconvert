/**
 * The install command for the system Tesseract the OCR note offers (design, resolved question 4):
 * the full command for the detected OS — Tesseract plus the German and Turkish packs — or none
 * where the OS has no package manager command to give. Commands, so not translated.
 */
export function tesseractCommand(os: string): string | null {
  switch (os) {
    case "macos":
      return "brew install tesseract tesseract-lang";
    case "linux":
      return "sudo apt install tesseract-ocr tesseract-ocr-deu tesseract-ocr-tur";
    default:
      return null;
  }
}
