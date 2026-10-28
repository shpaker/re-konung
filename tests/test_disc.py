from konung import disc


def test_disc_tracks_cover_whole_image(k1_rus_image):
    mds = next(k1_rus_image.glob("*.mds"))
    tracks = disc.read_tracks(mds)
    assert [t.number for t in tracks] == list(range(1, 12))
    assert not tracks[0].is_audio and all(t.is_audio for t in tracks[1:])
    last = tracks[-1]
    mdf = next(k1_rus_image.glob("*.mdf"))
    assert last.file_offset + last.length * last.sector_size == mdf.stat().st_size
