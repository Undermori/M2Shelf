export interface TmdbMovie {mediaType?:"MOVIE"|"TV";id:number;title:string;originalTitle:string;releaseDate:string|null;overview:string;posterPath:string|null;originalLanguage?:string|null;automaticPoster?:boolean;posterPolicyVersion?:number;alternativeTitles?:string[];}
export interface TmdbBinding {movie:TmdbMovie;active:boolean;coverCachePath:string|null;coverError:string|null;}
export interface TmdbSearch {movies:TmdbMovie[];snapshot:string;}
export interface TmdbMatchDiagnostic {nodeId:number;query:string;year:number|null;outcome:string;updatedAt:string;}
export interface TmdbCredentialStatus {source:'DEFAULT'|'PERSONAL';defaultAvailable:boolean;personalConfigured:boolean;check:'UNCHECKED'|'VERIFIED'|'AUTH_ERROR'|'RATE_LIMITED'|'NETWORK_ERROR'}
