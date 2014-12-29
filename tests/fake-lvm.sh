#! /bin/sh

Used by the sudo faker to avoid needing actual LVM data, or to really
run sudo.  It ignores its args entirely.

cat <<EOF
  LV|VG|Attr|LSize|Pool|Origin|Data%|Meta%|Move|Log|Cpy%Sync|Convert
  a64.home|pixel|Vwi-aotz--|55.00g|thinpool||38.47|||||
  boot.bu|pixel|Vwi-aotz--|600.00m|thinpool||25.75|||||
  boot.bu.2014.12.25|pixel|Vwi---tz-k|600.00m|thinpool|boot.bu||||||
  boot.bu.2014.12.27|pixel|Vwi---tz-k|600.00m|thinpool|boot.bu||||||
  home|pixel|Vwi-aotz--|100.00g|thinpool||4.91|||||
  home.2014.12.25|pixel|Vwi---tz-k|100.00g|thinpool|home||||||
  home.2014.12.27|pixel|Vwi---tz-k|100.00g|thinpool|home||||||
  language|pixel|Vwi-aotz--|40.00g|thinpool||9.94|||||
  language.2014.12.25|pixel|Vwi---tz-k|40.00g|thinpool|language||||||
  language.2014.12.27|pixel|Vwi---tz-k|40.00g|thinpool|language||||||
  root|pixel|Vwi-aotz--|50.00g|thinpool||13.80|||||
  root.2014.12.25|pixel|Vwi---tz-k|50.00g|thinpool|root||||||
  root.2014.12.27|pixel|Vwi---tz-k|50.00g|thinpool|root||||||
  steam|pixel|Vwi-aotz--|100.00g|thinpool||4.24|||||
  swap|pixel|-wi-ao----|8.00g||||||||
  thinpool|pixel|twi-a-tz--|564.09g|||7.74|5.26||||
EOF
