use super::*;

pub(super) struct Columns {
    pub(super) id: tantivy::columnar::Column<u64>,
    pub(super) place: tantivy::columnar::Column<u64>,
    pub(super) x: tantivy::columnar::Column<f64>,
    pub(super) y: tantivy::columnar::Column<f64>,
    pub(super) importance: tantivy::columnar::Column<u64>,
    /// The doc ranges of each kind, by `Kind as usize`. The builder writes each kind in long runs
    /// (10 to 20 per segment of a world index), so a lookup seeks its kind's runs and skips the
    /// postings of the others: a place's name words also match millions of streets.
    pub(super) runs: [Vec<std::ops::Range<u32>>; 3],
    /// Each document's `country` term, numbered in `Index::country_codes` (0: none). A country
    /// term has millions of postings that tantivy's skip list walks one block at a time in every
    /// lookup it intersects; this costs 2 bytes per document instead.
    pub(super) countries: Box<[u16]>,
}

impl Columns {
    pub(super) fn open(
        reader: &tantivy::SegmentReader,
        country: Field,
        codes: &mut HashMap<String, u16>,
    ) -> Result<Columns> {
        use tantivy::DocSet;
        let mut countries = vec![0u16; reader.max_doc() as usize];
        let inverted = reader.inverted_index(country)?;
        let mut terms = inverted.terms().stream()?;
        while terms.advance() {
            let next = u16::try_from(codes.len() + 1).context("too many country terms")?;
            let code = *codes
                .entry(String::from_utf8_lossy(terms.key()).into_owned())
                .or_insert(next);
            let mut postings =
                inverted.read_postings_from_terminfo(terms.value(), IndexRecordOption::Basic)?;
            let mut doc = postings.doc();
            while doc != tantivy::TERMINATED {
                countries[doc as usize] = code;
                doc = postings.advance();
            }
        }
        let f = reader.fast_fields();
        let (id, place) = (f.u64("id")?, f.u64("place")?);
        let mut runs: [Vec<std::ops::Range<u32>>; 3] = Default::default();
        for doc in 0..reader.max_doc() {
            let kind = match (place.index.has_value(doc), id.index.has_value(doc)) {
                (false, _) => Kind::Place,
                (true, true) => Kind::Street,
                (true, false) => Kind::Postcode,
            } as usize;
            match runs[kind].last_mut() {
                Some(run) if run.end == doc => run.end += 1,
                _ => runs[kind].push(doc..doc + 1),
            }
        }
        Ok(Columns {
            id,
            place,
            x: f.f64("x")?,
            y: f.f64("y")?,
            importance: f.u64("importance")?,
            runs,
            countries: countries.into(),
        })
    }
}

/// The documents of one kind in each segment, from its first run to its last: a lookup's first
/// clause. An intersection finds its first match when its scorer is made, before the collector
/// can seek the kind's runs, so a place's words that name no place were otherwise sought through
/// every street. Builds before indexes were ordered by kind interleave the runs, and this spans
/// almost every document there.
#[derive(Clone, Debug)]
pub(super) struct OfKind(
    pub(super) std::sync::Arc<[(tantivy::index::SegmentId, std::ops::Range<u32>)]>,
);

impl OfKind {
    pub(super) fn new(readers: &[tantivy::SegmentReader], columns: &[Columns], kind: Kind) -> Self {
        OfKind(
            readers
                .iter()
                .zip(columns)
                .map(|(r, c)| {
                    let runs = &c.runs[kind as usize];
                    let span = match (runs.first(), runs.last()) {
                        (Some(first), Some(last)) => first.start..last.end,
                        _ => 0..0,
                    };
                    (r.segment_id(), span)
                })
                .collect(),
        )
    }
}

impl tantivy::query::Query for OfKind {
    fn weight(
        &self,
        _: tantivy::query::EnableScoring<'_>,
    ) -> tantivy::Result<Box<dyn tantivy::query::Weight>> {
        Ok(Box::new(self.clone()))
    }
}

impl tantivy::query::Weight for OfKind {
    fn scorer(
        &self,
        reader: &tantivy::SegmentReader,
        _: tantivy::Score,
    ) -> tantivy::Result<Box<dyn tantivy::query::Scorer>> {
        let span = self
            .0
            .iter()
            .find(|(id, _)| *id == reader.segment_id())
            .map_or(0..0, |(_, span)| span.clone());
        Ok(Box::new(Span {
            doc: if span.is_empty() {
                tantivy::TERMINATED
            } else {
                span.start
            },
            end: span.end,
        }))
    }

    fn explain(
        &self,
        _: &tantivy::SegmentReader,
        _: tantivy::DocId,
    ) -> tantivy::Result<tantivy::query::Explanation> {
        Err(tantivy::TantivyError::InvalidArgument(
            "OfKind does not score".into(),
        ))
    }
}

/// Every document from `doc` up to `end`: seeking costs nothing.
struct Span {
    doc: tantivy::DocId,
    end: tantivy::DocId,
}

impl tantivy::DocSet for Span {
    fn advance(&mut self) -> tantivy::DocId {
        self.seek(self.doc.saturating_add(1))
    }

    fn seek(&mut self, target: tantivy::DocId) -> tantivy::DocId {
        self.doc = if target >= self.end {
            tantivy::TERMINATED
        } else {
            target.max(self.doc)
        };
        self.doc
    }

    fn doc(&self) -> tantivy::DocId {
        self.doc
    }

    fn size_hint(&self) -> u32 {
        self.end.saturating_sub(self.doc)
    }
}

impl tantivy::query::Scorer for Span {
    fn score(&mut self) -> tantivy::Score {
        1.0
    }
}

/// The kind of a document: `kind` is a string field whose terms (about 40 million street
/// documents in a world index) are far longer than any name's, and intersecting with them walks
/// postings the query needs none of. The fast columns tell the kinds apart as well: places have
/// no `place`, streets have both `place` and `id`, postcodes have only `place`.
#[derive(Clone, Copy, Debug)]
pub(super) enum Kind {
    Place,
    Street,
    Postcode,
}

pub(super) struct ByImportance<'a> {
    pub(super) n: usize,
    pub(super) kind: Kind,
    pub(super) country: Option<u16>,
    /// Stop at the first match: whether there is one, not which is most important.
    pub(super) any: bool,
    pub(super) columns: &'a [Columns],
}

/// What separates documents of equal importance: their id, then their coordinates. Document
/// order in an index depends on how many threads wrote it, so it must not decide which of two
/// equally important places a lookup finds first.
type Tie = (u64, u64, u64);

type Ranked = (u64, std::cmp::Reverse<Tie>, u32);

pub(super) struct SegmentTop {
    pub(super) segment: u32,
    pub(super) n: usize,
    pub(super) importance: tantivy::columnar::Column<u64>,
    pub(super) id: tantivy::columnar::Column<u64>,
    pub(super) x: tantivy::columnar::Column<f64>,
    pub(super) y: tantivy::columnar::Column<f64>,
    pub(super) heap: std::collections::BinaryHeap<std::cmp::Reverse<Ranked>>,
}

impl SegmentTop {
    /// Places and streets have an id of their own, so one column read decides; postcode documents
    /// have none and fall back to their coordinates.
    pub(super) fn tie(&self, doc: u32) -> Tie {
        match self.id.first(doc) {
            Some(id) => (id, 0, 0),
            None => (
                0,
                self.x.first(doc).map_or(0, f64::to_bits),
                self.y.first(doc).map_or(0, f64::to_bits),
            ),
        }
    }
}

impl tantivy::collector::Collector for ByImportance<'_> {
    type Fruit = Vec<DocAddress>;
    type Child = SegmentTop;

    fn for_segment(&self, segment: u32, _: &tantivy::SegmentReader) -> tantivy::Result<SegmentTop> {
        let c = &self.columns[segment as usize];
        Ok(SegmentTop {
            segment,
            n: self.n,
            importance: c.importance.clone(),
            id: c.id.clone(),
            x: c.x.clone(),
            y: c.y.clone(),
            heap: Default::default(),
        })
    }

    fn requires_scoring(&self) -> bool {
        false
    }

    fn collect_segment(
        &self,
        weight: &dyn tantivy::query::Weight,
        segment: u32,
        reader: &tantivy::SegmentReader,
    ) -> tantivy::Result<Vec<(u64, Tie, DocAddress)>> {
        use tantivy::{DocSet, collector::SegmentCollector};
        let mut top = self.for_segment(segment, reader)?;
        let alive = reader.alive_bitset();
        let mut scorer = weight.scorer(reader, 1.0)?;
        let columns = &self.columns[segment as usize];
        for run in &columns.runs[self.kind as usize] {
            let mut doc = scorer.doc();
            if doc < run.start {
                doc = scorer.seek(run.start);
            }
            while doc < run.end {
                if alive.is_none_or(|a| a.is_alive(doc))
                    && self
                        .country
                        .is_none_or(|c| columns.countries[doc as usize] == c)
                {
                    top.collect(doc, 0.0);
                    if self.any {
                        return Ok(top.harvest());
                    }
                }
                doc = scorer.advance();
            }
            if doc == tantivy::TERMINATED {
                break;
            }
        }
        Ok(top.harvest())
    }

    fn merge_fruits(
        &self,
        fruits: Vec<Vec<(u64, Tie, DocAddress)>>,
    ) -> tantivy::Result<Vec<DocAddress>> {
        let mut all: Vec<(u64, Tie, DocAddress)> = fruits.into_iter().flatten().collect();
        all.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
        Ok(all.into_iter().take(self.n).map(|(_, _, a)| a).collect())
    }
}

impl tantivy::collector::SegmentCollector for SegmentTop {
    type Fruit = Vec<(u64, Tie, DocAddress)>;

    fn collect(&mut self, doc: u32, _: tantivy::Score) {
        let v = self.importance.first(doc).unwrap_or(0);
        let full = self.heap.len() >= self.n;
        // a less important document needs no tie-break
        if full && self.heap.peek().is_some_and(|m| m.0.0 > v) {
            return;
        }
        let ranked = std::cmp::Reverse((v, std::cmp::Reverse(self.tie(doc)), doc));
        if full {
            if self.heap.peek().is_some_and(|m| *m <= ranked) {
                return;
            }
            self.heap.pop();
        }
        self.heap.push(ranked);
    }

    fn harvest(self) -> Self::Fruit {
        let segment = self.segment;
        self.heap
            .into_iter()
            .map(|std::cmp::Reverse((v, std::cmp::Reverse(tie), doc))| {
                (v, tie, DocAddress::new(segment, doc))
            })
            .collect()
    }
}
