export interface TextReaderSettings {
  mode: 'PAGED'|'SCROLL'; theme: 'APP'|'PAPER'|'SEPIA'|'NIGHT'|'CUSTOM';
  fontFamily: 'SYSTEM'|'SERIF'|'MONO'; fontSize: number; fontWeight: number; italic: boolean;
  alignment: 'left'|'center'|'right'|'justify'; lineHeight: number; paragraphSpacing: number;
  letterSpacing: number; wordSpacing: number; maxWidth: number; horizontalMargin: number; verticalMargin: number;
  backgroundColor: string; textColor: string;
  brightness: number; contrast: number; saturation: number; sepia: number; hue: number; negative: boolean;
}
export const defaultTextReaderSettings: TextReaderSettings = {
  mode: 'PAGED', theme: 'APP', fontFamily: 'SYSTEM', fontSize: 22, fontWeight: 400, italic: false,
  alignment: 'left', lineHeight: 1.8, paragraphSpacing: .7, letterSpacing: 0, wordSpacing: 0,
  maxWidth: 800, horizontalMargin: 32, verticalMargin: 24, backgroundColor: '#f5eedf', textColor: '#302b25',
  brightness: 100, contrast: 100, saturation: 100, sepia: 0, hue: 0, negative: false,
};
