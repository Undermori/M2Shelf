import type {ComicBook} from './comic';
import type {ComicReaderSettings} from './comic';

export interface ReaderSessionPreferences {
  comicSettings?:ComicReaderSettings;
  fit:'PAGE'|'WIDTH'|'NATIVE';
  zoom:number;
  background:string;
}

export interface NextVolumeResult {
  status:string;
  current:ComicBook;
  next:ComicBook|null;
  currentVolume:number|null;
  nextVolume:number|null;
  snapshot:string;
}
