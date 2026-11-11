ALTER TABLE nodes ADD COLUMN manual_display_name INTEGER NOT NULL DEFAULT 0 CHECK(manual_display_name IN (0,1));

-- Earlier releases stored renamed titles without recording the user's override intent.
UPDATE nodes SET manual_display_name=1
WHERE trim(display_name)<>'' AND display_name<>folder_name;
