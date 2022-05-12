use alloc::alloc::{AllocError, Allocator, Layout};
use core::any::TypeId;
use core::fmt::{self, Debug, Display};
use core::hash::{Hash, Hasher};
use core::marker::PhantomData;
use core::mem::MaybeUninit;
use core::ptr::NonNull;

use liblumen_alloc::gc::GcBox;
use liblumen_alloc::rc::RcBox;

use super::{OpaqueTerm, Term};

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum CharlistToBinaryError {
    /// The list isn't a charlist and/or is an improper list
    InvalidList,
    /// Could not allocate enough memory to store the binary
    AllocError,
}

#[derive(Copy, Clone)]
pub struct Cons {
    head: OpaqueTerm,
    tail: OpaqueTerm,
}
impl Cons {
    pub const TYPE_ID: TypeId = TypeId::of::<Cons>();

    /// Allocates a new cons cell in the given allocator
    ///
    /// NOTE: The returned cell is wrapped in `MaybeUninit<T>` because the head/tail require
    /// initialization.
    pub fn new_in<A: Allocator>(alloc: A) -> Result<NonNull<MaybeUninit<Cons>>, AllocError> {
        alloc.allocate(Layout::<Cons>::new()).map(|ptr| ptr.cast())
    }

    /// Constructs a list from the given slice, the output of which will be in the same order as the slice.
    pub fn from_slice<A: Allocator>(slice: &[Term], alloc: A) -> Result<NonNull<Cons>, AllocError> {
        let mut builder = ListBuilder::new(alloc);
        for value in slice.iter().rev() {
            builder.push(value)?;
        }
        builder.finish()
    }
    /// During garbage collection, when a list cell is moved to the new heap, a
    /// move marker is left in the original location. For a cons cell, the move
    /// marker sets the first word to None, and the second word to a pointer to
    /// the new location.
    #[inline]
    pub fn is_move_marker(&self) -> bool {
        self.head.is_null()
    }

    /// Returns the head of this list as a Term
    pub fn head(&self) -> Term {
        self.head.into()
    }

    /// Returns the tail of this list as a Term
    ///
    /// NOTE: If the tail of this cell is _not_ Nil or Cons, it represents an improper list
    pub fn tail(&self) -> Term {
        self.tail.into()
    }

    /// Constructs a new cons cell with the given head/tail values
    #[inline]
    pub fn cons(head: Term, tail: Term) -> Cons {
        Self {
            head: head.into(),
            tail: tail.into(),
        }
    }

    /// Traverse the list, producing a `Result<Term, ImproperList>` for each element.
    ///
    /// If the list is proper, all elements will be `Ok(Term)`, but if the list is improper,
    /// the last element produced will be `Err(ImproperList)`. This can be unwrapped to get at
    /// the contained value, or treated as an error, depending on the context.
    #[inline]
    pub fn iter(&self) -> Iter<'_> {
        Iter::new(self)
    }

    /// Returns true if this cell is the head of a proper list.
    ///
    /// NOTE: The cost of this function is linear in the length of the list (i.e. `O(N)`)
    pub fn is_proper(&self) -> bool {
        self.iter().all(|result| result.is_ok())
    }

    /// Searches this keyword list for the first element which has a matching key
    /// at the given index.
    ///
    /// If no key is found, returns 'badarg'
    pub fn keyfind<I, K: Into<Term>>(&self, index: I, key: K) -> anyhow::Result<Option<Term>>
    where
        I: TupleIndex + Copy,
    {
        let key = key.into();
        for result in self.iter() {
            let Term::Tuple(tup) = result? else { continue; };
            let Ok(candidate) = tup.get_element(index) else { continue; };
            if candidate == key {
                return Ok(Some(Term::Tuple(tup)));
            }
        }

        Ok(None)
    }
}

// Charlists
impl Cons {
    /// Constructs a charlist from the given string
    pub fn charlist_from_str<A: Allocator>(s: &str, alloc: A) -> Result<NonNull<Cons>, AllocError> {
        let mut builder = ListBuilder::new(alloc);
        for c in s.chars().rev() {
            builder.push(Term::Int((c as u32) as i64))?;
        }
        builder.finish()
    }

    /// Converts a charlist to a binary value.
    ///
    /// NOTE: This function will return an error if the list is not a charlist. It will also return
    /// an error if we are unable to allocate memory for the binary.
    pub fn charlist_to_binary<A: Allocator>(
        &self,
        alloc: A,
    ) -> Result<Term, CharlistToBinaryError> {
        // We need to know whether or not the resulting binary should be allocated in `alloc`,
        // or on the global heap as a reference-counted binary. We also want to determine the target
        // encoding. So we'll scan the list twice, once to gather the size in bytes + encoding, the second
        // to write each byte to the allocated region.
        let (len, encoding) = self
            .get_charlist_size_and_encoding()
            .ok_or_else(|| CharlistToBinaryError::InvalidList)?;
        if len < 64 {
            self.charlist_to_heap_binary(len, encoding, alloc)
        } else {
            self.charlist_to_refc_binary(len, encoding)
        }
    }

    /// Writes this charlist to a GcBox, i.e. allocates on a process heap
    fn charlist_to_heap_binary<A: Allocator>(&self, len: usize, encoding: Encoding, alloc: A) -> Result<Term, CharlistToBinaryError> {
        let mut gcbox = GcBox::<BinaryData>::with_capacity_in(len, alloc).map_err(|_| CharlistToBinaryError::AllocError)?;
        {
            let value = unsafe { GcBox::get_mut_unchecked(&mut gcbox) };
            value.flags = BinaryFlags::new(encoding);
            let mut writer = value.write();
            if encoding == Encoding::Utf8 {
                self.write_unicode_charlist_to_buffer(&mut writer).unwrap();
            } else {
                self.write_raw_charlist_to_buffer(&mut writer);
            }
        }
        Ok(gcbox.into())
    }

    /// Writes this charlist to an RcBox, i.e. allocates on the global heap
    fn charlist_to_refc_binary(
        &self,
        len: usize,
        encoding: Encoding,
    ) -> Result<Term, CharlistToBinaryError> {
        let mut rcbox = RcBox::<BinaryData>::with_capacity(len);
        {
            let value = unsafe { RcBox::get_mut_unchecked(&mut rcbox) };
            value.flags = BinaryFlags::new(encoding);
            let mut writer = value.write();
            if encoding == Encoding::Utf8 {
                self.write_unicode_charlist_to_buffer(&mut writer).unwrap();
            } else {
                self.write_raw_charlist_to_buffer(&mut writer);
            }
        }
        Ok(rcbox.into())
    }

    /// Writes this charlist codepoint-by-codepoint to a buffer via the provided writer
    ///
    /// By the time this has called, we should already have validated that the list is valid unicode codepoints,
    /// and that the binary we've allocated has enough raw bytes to hold the contents of this charlist. This
    /// should not be called directly otherwise.
    fn write_unicode_charlist_to_buffer<W: fmt::Write>(&self, writer: &mut W) {
        for element in self.iter() {
            let Ok(Term::Int(codepoint)) = element else { return Err(CharlistToBinary::InvalidList.into()); }
            let codepoint = codepoint.try_into().unwrap();
            let c = unsafe { char::from_u32_unchecked(codepoint) };
            writer.write_char(c).unwrap()
        }
    }

    /// Same as `write_unicode_charlist_to_buffer`, but for ASCII charlists, which is slightly more efficient
    /// since we can skip the unicode conversion overhead.
    fn write_raw_charlist_to_buffer(&self, writer: &mut BinaryWriter<'_>) {
        for element in self.iter() {
            let Ok(Term::Int(byte)) = element else { return Err(CharlistToBinary::InvalidList) };
            writer.push_byte(byte.try_into().unwrap());
        }
    }

    /// This function walks the entire list, calculating the total bytes required to hold all of the characters,
    /// as well as what encoding is suitable for the charlist.
    ///
    /// If this list is not a charlist, or is an improper list, None is returned.
    fn get_charlist_size_and_encoding(&self) -> Option<(usize, Encoding)> {
        let mut len = 0;
        let mut encoding = Encoding::Utf8;
        for element in self.iter() {
            match element.map_err(|_| CharlistToBinaryError::InvalidList)? {
                Term::Int(codepoint) => match encoding {
                    // If we think we have a valid utf-8 charlist, we do some extra validation
                    Encoding::Utf8 => {
                        match codepoint.try_into() {
                            Ok(codepoint) => match char::from_u32(codepoint) {
                                Some(c) => {
                                    len += len_utf8(codepoint);
                                }
                                None if codepoint > 255 => {
                                    // Invalid UTF-8 codepoint and not a valid byte value, this isn't a charlist
                                    return Err(CharlistToBinaryError::InvalidList);
                                }
                                None => {
                                    // This is either a valid latin1 codepoint, or a plain byte, determine which,
                                    // as in both cases we need to update the encoding
                                    len += 1;
                                    if Encoding::is_latin1_byte(codepoint.try_into().unwrap()) {
                                        encoding = Encoding::Latin1;
                                    } else {
                                        encoding = Encoding::Raw;
                                    }
                                }
                            },
                            // The codepoint exceeds the valid range for u32, cannot be a charlist
                            Err(_) => return Err(CharlistToBinaryError::InvalidList),
                        }
                    }
                    // Likewise for Latin1
                    Encoding::Latin1 => {
                        if codepoint > 255 {
                            return Err(CharlistToBinaryError::InvalidList);
                        }
                        len += 1;
                        if !Encoding::is_latin1_byte(codepoint.try_into().unwrap()) {
                            encoding = Encoding::Raw;
                        }
                    }
                    Encoding::Raw => {
                        if codepoint > 255 {
                            return Err(CharlistToBinaryError::InvalidList);
                        }
                        len += 1;
                    }
                },
                _ => return Err(CharlistToBinaryError::InvalidList),
            }
        }

        Some((len, encoding))
    }

    // See https://github.com/erlang/otp/blob/b8e11b6abe73b5f6306e8833511fcffdb9d252b5/erts/emulator/beam/erl_printf_term.c#L117-L140
    fn is_printable_string(&self) -> bool {
        self.iter().all(|result| match result {
            Ok(element) => {
                // See https://github.com/erlang/otp/blob/b8e11b6abe73b5f6306e8833511fcffdb9d252b5/erts/emulator/beam/erl_printf_term.c#L128-L129
                let Ok(c) = char::try_from(element) else { return false; };
                // https://github.com/erlang/otp/blob/b8e11b6abe73b5f6306e8833511fcffdb9d252b5/erts/emulator/beam/erl_printf_term.c#L132
                c.is_ascii_graphic() || c.is_ascii_whitespace()
            }
            _ => false,
        })
    }
}

impl Eq for Cons {}
impl PartialEq for Cons {
    fn eq(&self, other: &Self) -> bool {
        self.head().eq(&other.head()) && self.tail().eq(&other.tail())
    }
}
impl PartialOrd for Cons {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Cons {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.iter().cmp(other.iter())
    }
}
impl Hash for Cons {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for item in self.iter() {
            item.hash(state);
        }
    }
}
impl Debug for Cons {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_char('[')?;
        for (i, value) in self.iter().enumerate() {
            match value {
                Ok(value) if i > 0 => write!(f, ", {:?}", value)?,
                Ok(value) => write!(f, "{:?}", value)?,
                Err(improper) if i > 0 => write!(f, " | {:?}", improper)?,
                Err(improper) => write!(f, "{:?}", improper)?,
            }
        }
        f.write_char(']')
    }
}
impl Display for Cons {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // See https://github.com/erlang/otp/blob/b8e11b6abe73b5f6306e8833511fcffdb9d252b5/erts/emulator/beam/erl_printf_term.c#L423-443
        if self.is_printable_string() {
            f.write_char('\"')?;

            for result in self.iter() {
                // `is_printable_string` guarantees all Ok
                let element = result.unwrap();
                match element.try_into().unwrap() {
                    '\n' => f.write_str("\\\n")?,
                    '\"' => f.write_str("\\\"")?,
                    c => f.write_char(c)?,
                }
            }

            f.write_char('\"')
        } else {
            f.write_char('[')?;

            for (i, value) in self.iter().enumerate() {
                match value {
                    Ok(value) if i > 0 => write!(f, ", {}", value)?,
                    Ok(value) => write!(f, "{}", value)?,
                    Err(improper) if i > 0 => write!(f, " | {}", improper)?,
                    Err(improper) => write!(f, "{}", improper)?,
                }
            }

            f.write_char(']')
        }
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ImproperList {
    pub tail: Term,
}
impl Debug for ImproperList {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Debug::fmt(&self.tail, f)
    }
}
impl Display for ImproperList {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Display::fmt(&self.tail, f)
    }
}

pub struct Iter<'a> {
    head: Option<Result<Term, ImproperList>>,
    tail: Option<OpaqueTerm>,
    _marker: PhantomData<&'a Cons>,
}
impl Iter<'_> {
    fn new(cons: &Cons) -> Self {
        Self {
            head: Some(Ok(cons.head())),
            tail: Some(cons.tail),
            _marker: PhantomData,
        }
    }
}

impl std::iter::FusedIterator for Iter<'_> {}

impl Iterator for Iter<'_> {
    type Item = Result<Term, ImproperList>;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self.head.take();

        match next {
            None => next,
            Some(Err(_)) => {
                self.tail = None;
                next
            }
            Some(Ok(Term::Nil)) if self.tail.is_none() => Some(Ok(Term::Nil)),
            _ => {
                let tail = self.tail.unwrap();
                match tail.into() {
                    Term::Nil => {
                        self.head = Some(Ok(Term::Nil));
                        self.tail = None;
                        next
                    }
                    Term::Cons(cons) => {
                        let cons = unsafe { &*cons };
                        self.head = Some(Ok(cons.head()));
                        self.tail = Some(cons.tail);
                        next
                    }
                    Term::None => panic!("invalid none value found in list"),
                    tail => {
                        self.head = Some(Err(ImproperList { tail }));
                        self.tail = None;
                        next
                    }
                }
            }
        }
    }
}

pub struct ListBuilder<'a, A: Allocator> {
    alloc: &'a mut A,
    head: Option<NonNull<Cons>>,
}
impl<'a, A: Allocator> ListBuilder<'a, A> {
    pub fn new(alloc: &'a mut A) -> Self {
        Self { alloc, head: None }
    }

    pub fn push(&mut self, value: Term) -> Result<(), AllocError> {
        let head = value.clone_into(&mut self.alloc)?;
        match self.head.take() {
            None => {
                let cell = Cons::new_in(&mut self.alloc)?;
                cell.as_mut().write(Cons {
                    head,
                    tail: OpaqueTerm::NIL,
                });
                self.head.insert(cell.cast());
            }
            Some(tail) => {
                let tail: OpaqueTerm = tail.into();
                let cell = Cons::new_in(&mut self.alloc)?;
                cell.as_mut().write(Cons { head, tail });
                self.head.insert(cell.cast());
            }
        }
    }

    pub fn finish(mut self) -> Option<NonNull<Cons>> {
        self.head.take()
    }
}

#[inline]
fn len_utf8(code: u32) -> usize {
    const MAX_ONE_B: u32 = 0x80;
    const MAX_TWO_B: u32 = 0x800;
    const MAX_THREE_B: u32 = 0x10000;

    if code < MAX_ONE_B {
        1
    } else if code < MAX_TWO_B {
        2
    } else if code < MAX_THREE_B {
        3
    } else {
        4
    }
}