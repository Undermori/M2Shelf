-- Extend the existing provider write boundary without rewriting bindings or media rows.
DROP TRIGGER tmdb_movie_scope;
DROP TRIGGER tmdb_movie_update_scope;

CREATE TRIGGER tmdb_movie_scope BEFORE INSERT ON tmdb_movie_bindings
 WHEN NOT EXISTS(
  SELECT 1 FROM nodes n JOIN library_roots r ON r.id=n.library_root_id
  WHERE n.id=NEW.node_id AND r.media_kind='VIDEO'
   AND r.video_subject_scope IN('LIVE_ACTION','MIXED')
   AND n.total_video_count>0
   AND (n.node_type IN('WORK','AUTO_WORK') OR
    (n.node_type='CONTAINER' AND n.parent_node_id IS NOT NULL AND COALESCE(json_extract(NEW.payload_json,'$.mediaType'),'MOVIE')='TV')))
 BEGIN SELECT RAISE(ABORT,'TMDB_REQUIRES_MOVIE_WORK'); END;

CREATE TRIGGER tmdb_movie_update_scope BEFORE UPDATE OF node_id,movie_id,payload_json,active ON tmdb_movie_bindings
 WHEN NEW.active=1 AND NOT EXISTS(
  SELECT 1 FROM nodes n JOIN library_roots r ON r.id=n.library_root_id
  WHERE n.id=NEW.node_id AND r.media_kind='VIDEO'
   AND r.video_subject_scope IN('LIVE_ACTION','MIXED')
   AND n.total_video_count>0
   AND (n.node_type IN('WORK','AUTO_WORK') OR
    (n.node_type='CONTAINER' AND n.parent_node_id IS NOT NULL AND COALESCE(json_extract(NEW.payload_json,'$.mediaType'),'MOVIE')='TV')))
 BEGIN SELECT RAISE(ABORT,'TMDB_REQUIRES_MOVIE_WORK'); END;
