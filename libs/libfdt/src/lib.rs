// Copyright 2022, The Android Open Source Project
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Wrapper around libfdt library. Provides parsing/generating functionality
//! to a bare-metal environment.

#![no_std]

mod iterators;

pub use iterators::{AddressRange, CellIterator, MemRegIterator, RangesIterator, Reg, RegIterator};

use core::ffi::{c_int, c_void, CStr};
use core::fmt;
use core::mem;
use core::result;

/// Error type corresponding to libfdt error codes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FdtError {
    /// FDT_ERR_NOTFOUND
    NotFound,
    /// FDT_ERR_EXISTS
    Exists,
    /// FDT_ERR_NOSPACE
    NoSpace,
    /// FDT_ERR_BADOFFSET
    BadOffset,
    /// FDT_ERR_BADPATH
    BadPath,
    /// FDT_ERR_BADPHANDLE
    BadPhandle,
    /// FDT_ERR_BADSTATE
    BadState,
    /// FDT_ERR_TRUNCATED
    Truncated,
    /// FDT_ERR_BADMAGIC
    BadMagic,
    /// FDT_ERR_BADVERSION
    BadVersion,
    /// FDT_ERR_BADSTRUCTURE
    BadStructure,
    /// FDT_ERR_BADLAYOUT
    BadLayout,
    /// FDT_ERR_INTERNAL
    Internal,
    /// FDT_ERR_BADNCELLS
    BadNCells,
    /// FDT_ERR_BADVALUE
    BadValue,
    /// FDT_ERR_BADOVERLAY
    BadOverlay,
    /// FDT_ERR_NOPHANDLES
    NoPhandles,
    /// FDT_ERR_BADFLAGS
    BadFlags,
    /// FDT_ERR_ALIGNMENT
    Alignment,
    /// Unexpected error code
    Unknown(i32),
}

impl fmt::Display for FdtError {
    /// Prints error messages from libfdt.h documentation.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::NotFound => write!(f, "The requested node or property does not exist"),
            Self::Exists => write!(f, "Attempted to create an existing node or property"),
            Self::NoSpace => write!(f, "Insufficient buffer space to contain the expanded tree"),
            Self::BadOffset => write!(f, "Structure block offset is out-of-bounds or invalid"),
            Self::BadPath => write!(f, "Badly formatted path"),
            Self::BadPhandle => write!(f, "Invalid phandle length or value"),
            Self::BadState => write!(f, "Received incomplete device tree"),
            Self::Truncated => write!(f, "Device tree or sub-block is improperly terminated"),
            Self::BadMagic => write!(f, "Device tree header missing its magic number"),
            Self::BadVersion => write!(f, "Device tree has a version which can't be handled"),
            Self::BadStructure => write!(f, "Device tree has a corrupt structure block"),
            Self::BadLayout => write!(f, "Device tree sub-blocks in unsupported order"),
            Self::Internal => write!(f, "libfdt has failed an internal assertion"),
            Self::BadNCells => write!(f, "Bad format or value of #address-cells or #size-cells"),
            Self::BadValue => write!(f, "Unexpected property value"),
            Self::BadOverlay => write!(f, "Overlay cannot be applied"),
            Self::NoPhandles => write!(f, "Device tree doesn't have any phandle available anymore"),
            Self::BadFlags => write!(f, "Invalid flag or invalid combination of flags"),
            Self::Alignment => write!(f, "Device tree base address is not 8-byte aligned"),
            Self::Unknown(e) => write!(f, "Unknown libfdt error '{e}'"),
        }
    }
}

/// Result type with FdtError enum.
pub type Result<T> = result::Result<T, FdtError>;

fn fdt_err(val: c_int) -> Result<c_int> {
    if val >= 0 {
        Ok(val)
    } else {
        Err(match -val as _ {
            libfdt_bindgen::FDT_ERR_NOTFOUND => FdtError::NotFound,
            libfdt_bindgen::FDT_ERR_EXISTS => FdtError::Exists,
            libfdt_bindgen::FDT_ERR_NOSPACE => FdtError::NoSpace,
            libfdt_bindgen::FDT_ERR_BADOFFSET => FdtError::BadOffset,
            libfdt_bindgen::FDT_ERR_BADPATH => FdtError::BadPath,
            libfdt_bindgen::FDT_ERR_BADPHANDLE => FdtError::BadPhandle,
            libfdt_bindgen::FDT_ERR_BADSTATE => FdtError::BadState,
            libfdt_bindgen::FDT_ERR_TRUNCATED => FdtError::Truncated,
            libfdt_bindgen::FDT_ERR_BADMAGIC => FdtError::BadMagic,
            libfdt_bindgen::FDT_ERR_BADVERSION => FdtError::BadVersion,
            libfdt_bindgen::FDT_ERR_BADSTRUCTURE => FdtError::BadStructure,
            libfdt_bindgen::FDT_ERR_BADLAYOUT => FdtError::BadLayout,
            libfdt_bindgen::FDT_ERR_INTERNAL => FdtError::Internal,
            libfdt_bindgen::FDT_ERR_BADNCELLS => FdtError::BadNCells,
            libfdt_bindgen::FDT_ERR_BADVALUE => FdtError::BadValue,
            libfdt_bindgen::FDT_ERR_BADOVERLAY => FdtError::BadOverlay,
            libfdt_bindgen::FDT_ERR_NOPHANDLES => FdtError::NoPhandles,
            libfdt_bindgen::FDT_ERR_BADFLAGS => FdtError::BadFlags,
            libfdt_bindgen::FDT_ERR_ALIGNMENT => FdtError::Alignment,
            _ => FdtError::Unknown(val),
        })
    }
}

fn fdt_err_expect_zero(val: c_int) -> Result<()> {
    match fdt_err(val)? {
        0 => Ok(()),
        _ => Err(FdtError::Unknown(val)),
    }
}

fn fdt_err_or_option(val: c_int) -> Result<Option<c_int>> {
    match fdt_err(val) {
        Ok(val) => Ok(Some(val)),
        Err(FdtError::NotFound) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Value of a #address-cells property.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum AddrCells {
    Single = 1,
    Double = 2,
    Triple = 3,
}

impl TryFrom<c_int> for AddrCells {
    type Error = FdtError;

    fn try_from(res: c_int) -> Result<Self> {
        match fdt_err(res)? {
            x if x == Self::Single as c_int => Ok(Self::Single),
            x if x == Self::Double as c_int => Ok(Self::Double),
            x if x == Self::Triple as c_int => Ok(Self::Triple),
            _ => Err(FdtError::BadNCells),
        }
    }
}

/// Value of a #size-cells property.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum SizeCells {
    None = 0,
    Single = 1,
    Double = 2,
}

impl TryFrom<c_int> for SizeCells {
    type Error = FdtError;

    fn try_from(res: c_int) -> Result<Self> {
        match fdt_err(res)? {
            x if x == Self::None as c_int => Ok(Self::None),
            x if x == Self::Single as c_int => Ok(Self::Single),
            x if x == Self::Double as c_int => Ok(Self::Double),
            _ => Err(FdtError::BadNCells),
        }
    }
}

/// DT node.
#[derive(Clone, Copy)]
pub struct FdtNode<'a> {
    fdt: &'a Fdt,
    offset: c_int,
}

impl<'a> FdtNode<'a> {
    /// Find parent node.
    pub fn parent(&self) -> Result<Self> {
        // SAFETY - Accesses (read-only) are constrained to the DT totalsize.
        let ret = unsafe { libfdt_bindgen::fdt_parent_offset(self.fdt.as_ptr(), self.offset) };

        Ok(Self { fdt: self.fdt, offset: fdt_err(ret)? })
    }

    /// Retrieve the standard (deprecated) device_type <string> property.
    pub fn device_type(&self) -> Result<Option<&CStr>> {
        self.getprop_str(CStr::from_bytes_with_nul(b"device_type\0").unwrap())
    }

    /// Retrieve the standard reg <prop-encoded-array> property.
    pub fn reg(&self) -> Result<Option<RegIterator<'a>>> {
        let reg = CStr::from_bytes_with_nul(b"reg\0").unwrap();

        if let Some(cells) = self.getprop_cells(reg)? {
            let parent = self.parent()?;

            let addr_cells = parent.address_cells()?;
            let size_cells = parent.size_cells()?;

            Ok(Some(RegIterator::new(cells, addr_cells, size_cells)))
        } else {
            Ok(None)
        }
    }

    /// Retrieves the standard ranges property.
    pub fn ranges<A, P, S>(&self) -> Result<Option<RangesIterator<'a, A, P, S>>> {
        let ranges = CStr::from_bytes_with_nul(b"ranges\0").unwrap();
        if let Some(cells) = self.getprop_cells(ranges)? {
            let parent = self.parent()?;
            let addr_cells = self.address_cells()?;
            let parent_addr_cells = parent.address_cells()?;
            let size_cells = self.size_cells()?;
            Ok(Some(RangesIterator::<A, P, S>::new(
                cells,
                addr_cells,
                parent_addr_cells,
                size_cells,
            )))
        } else {
            Ok(None)
        }
    }

    /// Retrieve the value of a given <string> property.
    pub fn getprop_str(&self, name: &CStr) -> Result<Option<&CStr>> {
        let value = if let Some(bytes) = self.getprop(name)? {
            Some(CStr::from_bytes_with_nul(bytes).map_err(|_| FdtError::BadValue)?)
        } else {
            None
        };
        Ok(value)
    }

    /// Retrieve the value of a given property as an array of cells.
    pub fn getprop_cells(&self, name: &CStr) -> Result<Option<CellIterator<'a>>> {
        if let Some(cells) = self.getprop(name)? {
            Ok(Some(CellIterator::new(cells)))
        } else {
            Ok(None)
        }
    }

    /// Retrieve the value of a given <u32> property.
    pub fn getprop_u32(&self, name: &CStr) -> Result<Option<u32>> {
        let value = if let Some(bytes) = self.getprop(name)? {
            Some(u32::from_be_bytes(bytes.try_into().map_err(|_| FdtError::BadValue)?))
        } else {
            None
        };
        Ok(value)
    }

    /// Retrieve the value of a given <u64> property.
    pub fn getprop_u64(&self, name: &CStr) -> Result<Option<u64>> {
        let value = if let Some(bytes) = self.getprop(name)? {
            Some(u64::from_be_bytes(bytes.try_into().map_err(|_| FdtError::BadValue)?))
        } else {
            None
        };
        Ok(value)
    }

    /// Retrieve the value of a given property.
    pub fn getprop(&self, name: &CStr) -> Result<Option<&'a [u8]>> {
        let mut len: i32 = 0;
        // SAFETY - Accesses are constrained to the DT totalsize (validated by ctor) and the
        // function respects the passed number of characters.
        let prop = unsafe {
            libfdt_bindgen::fdt_getprop_namelen(
                self.fdt.as_ptr(),
                self.offset,
                name.as_ptr(),
                // *_namelen functions don't include the trailing nul terminator in 'len'.
                name.to_bytes().len().try_into().map_err(|_| FdtError::BadPath)?,
                &mut len as *mut i32,
            )
        } as *const u8;

        let Some(len) = fdt_err_or_option(len)? else {
            return Ok(None); // Property was not found.
        };
        let len = usize::try_from(len).map_err(|_| FdtError::Internal)?;

        if prop.is_null() {
            // We expected an error code in len but still received a valid value?!
            return Err(FdtError::Internal);
        }

        let offset =
            (prop as usize).checked_sub(self.fdt.as_ptr() as usize).ok_or(FdtError::Internal)?;

        Ok(Some(self.fdt.buffer.get(offset..(offset + len)).ok_or(FdtError::Internal)?))
    }

    /// Get reference to the containing device tree.
    pub fn fdt(&self) -> &Fdt {
        self.fdt
    }

    fn next_compatible(self, compatible: &CStr) -> Result<Option<Self>> {
        // SAFETY - Accesses (read-only) are constrained to the DT totalsize.
        let ret = unsafe {
            libfdt_bindgen::fdt_node_offset_by_compatible(
                self.fdt.as_ptr(),
                self.offset,
                compatible.as_ptr(),
            )
        };

        Ok(fdt_err_or_option(ret)?.map(|offset| Self { fdt: self.fdt, offset }))
    }

    fn address_cells(&self) -> Result<AddrCells> {
        // SAFETY - Accesses are constrained to the DT totalsize (validated by ctor).
        unsafe { libfdt_bindgen::fdt_address_cells(self.fdt.as_ptr(), self.offset) }
            .try_into()
            .map_err(|_| FdtError::Internal)
    }

    fn size_cells(&self) -> Result<SizeCells> {
        // SAFETY - Accesses are constrained to the DT totalsize (validated by ctor).
        unsafe { libfdt_bindgen::fdt_size_cells(self.fdt.as_ptr(), self.offset) }
            .try_into()
            .map_err(|_| FdtError::Internal)
    }
}

/// Mutable FDT node.
pub struct FdtNodeMut<'a> {
    fdt: &'a mut Fdt,
    offset: c_int,
}

impl<'a> FdtNodeMut<'a> {
    /// Append a property name-value (possibly empty) pair to the given node.
    pub fn appendprop<T: AsRef<[u8]>>(&mut self, name: &CStr, value: &T) -> Result<()> {
        // SAFETY - Accesses are constrained to the DT totalsize (validated by ctor).
        let ret = unsafe {
            libfdt_bindgen::fdt_appendprop(
                self.fdt.as_mut_ptr(),
                self.offset,
                name.as_ptr(),
                value.as_ref().as_ptr().cast::<c_void>(),
                value.as_ref().len().try_into().map_err(|_| FdtError::BadValue)?,
            )
        };

        fdt_err_expect_zero(ret)
    }

    /// Append a (address, size) pair property to the given node.
    pub fn appendprop_addrrange(&mut self, name: &CStr, addr: u64, size: u64) -> Result<()> {
        // SAFETY - Accesses are constrained to the DT totalsize (validated by ctor).
        let ret = unsafe {
            libfdt_bindgen::fdt_appendprop_addrrange(
                self.fdt.as_mut_ptr(),
                self.parent()?.offset,
                self.offset,
                name.as_ptr(),
                addr,
                size,
            )
        };

        fdt_err_expect_zero(ret)
    }

    /// Create or change a property name-value pair to the given node.
    pub fn setprop(&mut self, name: &CStr, value: &[u8]) -> Result<()> {
        // SAFETY - New value size is constrained to the DT totalsize
        //          (validated by underlying libfdt).
        let ret = unsafe {
            libfdt_bindgen::fdt_setprop(
                self.fdt.as_mut_ptr(),
                self.offset,
                name.as_ptr(),
                value.as_ptr().cast::<c_void>(),
                value.len().try_into().map_err(|_| FdtError::BadValue)?,
            )
        };

        fdt_err_expect_zero(ret)
    }

    /// Get reference to the containing device tree.
    pub fn fdt(&mut self) -> &mut Fdt {
        self.fdt
    }

    /// Add a new subnode to the given node and return it as a FdtNodeMut on success.
    pub fn add_subnode(&'a mut self, name: &CStr) -> Result<Self> {
        // SAFETY - Accesses are constrained to the DT totalsize (validated by ctor).
        let ret = unsafe {
            libfdt_bindgen::fdt_add_subnode(self.fdt.as_mut_ptr(), self.offset, name.as_ptr())
        };

        Ok(Self { fdt: self.fdt, offset: fdt_err(ret)? })
    }

    fn parent(&'a self) -> Result<FdtNode<'a>> {
        // SAFETY - Accesses (read-only) are constrained to the DT totalsize.
        let ret = unsafe { libfdt_bindgen::fdt_parent_offset(self.fdt.as_ptr(), self.offset) };

        Ok(FdtNode { fdt: &*self.fdt, offset: fdt_err(ret)? })
    }
}

/// Iterator over nodes sharing a same compatible string.
pub struct CompatibleIterator<'a> {
    node: FdtNode<'a>,
    compatible: &'a CStr,
}

impl<'a> CompatibleIterator<'a> {
    fn new(fdt: &'a Fdt, compatible: &'a CStr) -> Result<Self> {
        let node = fdt.root()?;
        Ok(Self { node, compatible })
    }
}

impl<'a> Iterator for CompatibleIterator<'a> {
    type Item = FdtNode<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self.node.next_compatible(self.compatible).ok()?;

        if let Some(node) = next {
            self.node = node;
        }

        next
    }
}

/// Wrapper around low-level libfdt functions.
#[repr(transparent)]
pub struct Fdt {
    buffer: [u8],
}

impl Fdt {
    /// Wraps a slice containing a Flattened Device Tree.
    ///
    /// Fails if the FDT does not pass validation.
    pub fn from_slice(fdt: &[u8]) -> Result<&Self> {
        // SAFETY - The FDT will be validated before it is returned.
        let fdt = unsafe { Self::unchecked_from_slice(fdt) };
        fdt.check_full()?;
        Ok(fdt)
    }

    /// Wraps a mutable slice containing a Flattened Device Tree.
    ///
    /// Fails if the FDT does not pass validation.
    pub fn from_mut_slice(fdt: &mut [u8]) -> Result<&mut Self> {
        // SAFETY - The FDT will be validated before it is returned.
        let fdt = unsafe { Self::unchecked_from_mut_slice(fdt) };
        fdt.check_full()?;
        Ok(fdt)
    }

    /// Wraps a slice containing a Flattened Device Tree.
    ///
    /// # Safety
    ///
    /// The returned FDT might be invalid, only use on slices containing a valid DT.
    pub unsafe fn unchecked_from_slice(fdt: &[u8]) -> &Self {
        mem::transmute::<&[u8], &Self>(fdt)
    }

    /// Wraps a mutable slice containing a Flattened Device Tree.
    ///
    /// # Safety
    ///
    /// The returned FDT might be invalid, only use on slices containing a valid DT.
    pub unsafe fn unchecked_from_mut_slice(fdt: &mut [u8]) -> &mut Self {
        mem::transmute::<&mut [u8], &mut Self>(fdt)
    }

    /// Make the whole slice containing the DT available to libfdt.
    pub fn unpack(&mut self) -> Result<()> {
        // SAFETY - "Opens" the DT in-place (supported use-case) by updating its header and
        // internal structures to make use of the whole self.fdt slice but performs no accesses
        // outside of it and leaves the DT in a state that will be detected by other functions.
        let ret = unsafe {
            libfdt_bindgen::fdt_open_into(
                self.as_ptr(),
                self.as_mut_ptr(),
                self.capacity().try_into().map_err(|_| FdtError::Internal)?,
            )
        };
        fdt_err_expect_zero(ret)
    }

    /// Pack the DT to take a minimum amount of memory.
    ///
    /// Doesn't shrink the underlying memory slice.
    pub fn pack(&mut self) -> Result<()> {
        // SAFETY - "Closes" the DT in-place by updating its header and relocating its structs.
        let ret = unsafe { libfdt_bindgen::fdt_pack(self.as_mut_ptr()) };
        fdt_err_expect_zero(ret)
    }

    /// Applies a DT overlay on the base DT.
    ///
    /// # Safety
    ///
    /// On failure, the library corrupts the DT and overlay so both must be discarded.
    pub unsafe fn apply_overlay<'a>(&'a mut self, overlay: &'a mut Fdt) -> Result<&'a mut Self> {
        fdt_err_expect_zero(libfdt_bindgen::fdt_overlay_apply(
            self.as_mut_ptr(),
            overlay.as_mut_ptr(),
        ))?;
        Ok(self)
    }

    /// Return an iterator of memory banks specified the "/memory" node.
    ///
    /// NOTE: This does not support individual "/memory@XXXX" banks.
    pub fn memory(&self) -> Result<Option<MemRegIterator>> {
        let memory = CStr::from_bytes_with_nul(b"/memory\0").unwrap();
        let device_type = CStr::from_bytes_with_nul(b"memory\0").unwrap();

        if let Some(node) = self.node(memory)? {
            if node.device_type()? != Some(device_type) {
                return Err(FdtError::BadValue);
            }
            let reg = node.reg()?.ok_or(FdtError::BadValue)?;

            Ok(Some(MemRegIterator::new(reg)))
        } else {
            Ok(None)
        }
    }

    /// Retrieve the standard /chosen node.
    pub fn chosen(&self) -> Result<Option<FdtNode>> {
        self.node(CStr::from_bytes_with_nul(b"/chosen\0").unwrap())
    }

    /// Get the root node of the tree.
    pub fn root(&self) -> Result<FdtNode> {
        self.node(CStr::from_bytes_with_nul(b"/\0").unwrap())?.ok_or(FdtError::Internal)
    }

    /// Find a tree node by its full path.
    pub fn node(&self, path: &CStr) -> Result<Option<FdtNode>> {
        Ok(self.path_offset(path)?.map(|offset| FdtNode { fdt: self, offset }))
    }

    /// Iterate over nodes with a given compatible string.
    pub fn compatible_nodes<'a>(&'a self, compatible: &'a CStr) -> Result<CompatibleIterator<'a>> {
        CompatibleIterator::new(self, compatible)
    }

    /// Get the mutable root node of the tree.
    pub fn root_mut(&mut self) -> Result<FdtNodeMut> {
        self.node_mut(CStr::from_bytes_with_nul(b"/\0").unwrap())?.ok_or(FdtError::Internal)
    }

    /// Find a mutable tree node by its full path.
    pub fn node_mut(&mut self, path: &CStr) -> Result<Option<FdtNodeMut>> {
        Ok(self.path_offset(path)?.map(|offset| FdtNodeMut { fdt: self, offset }))
    }

    /// Return the device tree as a slice (may be smaller than the containing buffer).
    pub fn as_slice(&self) -> &[u8] {
        &self.buffer[..self.totalsize()]
    }

    fn path_offset(&self, path: &CStr) -> Result<Option<c_int>> {
        let len = path.to_bytes().len().try_into().map_err(|_| FdtError::BadPath)?;
        // SAFETY - Accesses are constrained to the DT totalsize (validated by ctor) and the
        // function respects the passed number of characters.
        let ret = unsafe {
            // *_namelen functions don't include the trailing nul terminator in 'len'.
            libfdt_bindgen::fdt_path_offset_namelen(self.as_ptr(), path.as_ptr(), len)
        };

        fdt_err_or_option(ret)
    }

    fn check_full(&self) -> Result<()> {
        let len = self.buffer.len();
        // SAFETY - Only performs read accesses within the limits of the slice. If successful, this
        // call guarantees to other unsafe calls that the header contains a valid totalsize (w.r.t.
        // 'len' i.e. the self.fdt slice) that those C functions can use to perform bounds
        // checking. The library doesn't maintain an internal state (such as pointers) between
        // calls as it expects the client code to keep track of the objects (DT, nodes, ...).
        let ret = unsafe { libfdt_bindgen::fdt_check_full(self.as_ptr(), len) };
        fdt_err_expect_zero(ret)
    }

    fn as_ptr(&self) -> *const c_void {
        self as *const _ as *const c_void
    }

    fn as_mut_ptr(&mut self) -> *mut c_void {
        self as *mut _ as *mut c_void
    }

    fn capacity(&self) -> usize {
        self.buffer.len()
    }

    fn header(&self) -> &libfdt_bindgen::fdt_header {
        // SAFETY - A valid FDT (verified by constructor) must contain a valid fdt_header.
        unsafe { &*(&self as *const _ as *const libfdt_bindgen::fdt_header) }
    }

    fn totalsize(&self) -> usize {
        u32::from_be(self.header().totalsize) as usize
    }
}
