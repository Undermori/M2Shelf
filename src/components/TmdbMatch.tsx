import {M2ShelfError} from '../lib/api';
import type {TmdbMovie} from '../types/tmdb';
import type {BangumiSubject} from '../types/media';

/** Display rows retain the real, provider-specific binding DTO and snapshot. */
export type MatchResult =
 | {provider:'BANGUMI';id:number;title:string;originalTitle:string;date:string|null;poster:string|null;subject:BangumiSubject}
 | {provider:'TMDB';id:number;title:string;originalTitle:string;date:string|null;poster:string|null;snapshot:string;movie:TmdbMovie};
export function tmdbResult(movie:TmdbMovie,snapshot:string):MatchResult {
 return {provider:'TMDB',id:movie.id,title:movie.title,originalTitle:movie.originalTitle,date:movie.releaseDate,poster:movie.posterPath?`https://image.tmdb.org/t/p/w185${movie.posterPath}`:null,snapshot,movie};
}
export function tmdbErrorKey(value:unknown) {
 const code=String(value instanceof M2ShelfError?value.causeValue:value).match(/TMDB_[A-Z_]+(?:\d+)?/)?.[0];
 return code==='TMDB_HTTP_401'||code==='TMDB_HTTP_403'?'tmdb.authError':code==='TMDB_HTTP_404'?'tmdb.missingError':code==='TMDB_HTTP_429'?'tmdb.rateError':code==='TMDB_TARGET_STALE'?'tmdb.staleError':'tmdb.error';
}
